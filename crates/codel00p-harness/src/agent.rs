use std::sync::Arc;

use futures::future::join_all;

use crate::{
    errors::HarnessError,
    events::HarnessEvent,
    permissions::{AllowAllPermissionPolicy, PermissionPolicy, PermissionRequest},
    session::{SessionId, SessionState, TurnId, UserMessage},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    turn::{ExecutedToolCall, HarnessInferenceRequest, ModelClient, TurnOutcome},
    workspace::Workspace,
};
use codel00p_protocol::{ContextWindowState, EventId, RuntimeErrorKind};
use serde_json::json;

pub struct AgentHarness {
    model_client: Arc<dyn ModelClient>,
    workspace: Workspace,
    tools: ToolRegistry,
    permission_policy: Arc<dyn PermissionPolicy>,
    context_window: Option<ContextWindowState>,
    max_iterations: u32,
}

impl AgentHarness {
    pub fn builder() -> AgentHarnessBuilder {
        AgentHarnessBuilder::default()
    }

    pub async fn run_turn(
        self,
        session_id: SessionId,
        user_message: UserMessage,
    ) -> Result<TurnOutcome, HarnessError> {
        let turn_id = TurnId::new();
        let mut events = vec![HarnessEvent::TurnStarted {
            event_id: EventId::new(),
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
        }];
        let mut session_state = SessionState::new(session_id);
        session_state.push_user(user_message);
        events.push(HarnessEvent::ContextBuilt {
            event_id: EventId::new(),
            session_id: session_state.session_id().clone(),
            turn_id: turn_id.clone(),
            message_count: session_state.messages().len(),
        });

        let mut executed_tool_calls = Vec::new();

        for iteration in 1..=self.max_iterations {
            let mut request = HarnessInferenceRequest::new(session_state.clone())
                .with_runtime_context(
                    self.workspace.root().display().to_string(),
                    self.tools.names().into_iter().map(str::to_string).collect(),
                );
            if let Some(context_window) = &self.context_window {
                request = request.with_context_window(context_window.clone());
            }

            let response = self.model_client.infer(request).await?;

            events.push(HarnessEvent::InferenceRequested {
                event_id: EventId::new(),
                session_id: session_state.session_id().clone(),
                turn_id: turn_id.clone(),
                provider: response.provider().to_string(),
                model: response.model().to_string(),
            });
            events.push(HarnessEvent::InferenceCompleted {
                event_id: EventId::new(),
                session_id: session_state.session_id().clone(),
                turn_id: turn_id.clone(),
                finish_reason: response.finish_reason().map(str::to_string),
            });

            if response.tool_calls().is_empty() {
                let assistant_message = response.assistant_message().map(str::to_string);
                if let Some(content) = &assistant_message {
                    session_state.push_assistant(content);
                }

                events.push(HarnessEvent::TurnCompleted {
                    event_id: EventId::new(),
                    session_id: session_state.session_id().clone(),
                    turn_id,
                    iterations: iteration,
                });

                return Ok(TurnOutcome {
                    assistant_message,
                    tool_calls: executed_tool_calls,
                    events,
                    session_state,
                });
            }

            session_state.push_assistant_tool_calls(response.tool_calls().to_vec());

            let tool_calls = response.tool_calls();
            let mut index = 0;
            while index < tool_calls.len() {
                let mut end = index + 1;
                if self
                    .tools
                    .is_concurrency_safe(tool_calls[index].name(), tool_calls[index].input())
                {
                    while end < tool_calls.len()
                        && self
                            .tools
                            .is_concurrency_safe(tool_calls[end].name(), tool_calls[end].input())
                    {
                        end += 1;
                    }
                }

                let batch = &tool_calls[index..end];
                for tool_call in batch {
                    events.push(HarnessEvent::ToolCallRequested {
                        event_id: EventId::new(),
                        session_id: session_state.session_id().clone(),
                        turn_id: turn_id.clone(),
                        tool_name: tool_call.name().to_string(),
                    });
                }

                let permission_requests = batch
                    .iter()
                    .map(|tool_call| {
                        PermissionRequest::new(
                            format!("permission-{}", tool_call.id()),
                            session_state.session_id().clone(),
                            turn_id.clone(),
                            tool_call.name(),
                            tool_call.input().clone(),
                            self.tools
                                .permission_scope(tool_call.name(), tool_call.input()),
                        )
                    })
                    .collect::<Vec<_>>();

                for (tool_call, permission_request) in batch.iter().zip(&permission_requests) {
                    events.push(HarnessEvent::PermissionRequested {
                        event_id: EventId::new(),
                        session_id: session_state.session_id().clone(),
                        turn_id: turn_id.clone(),
                        tool_name: tool_call.name().to_string(),
                        request_id: permission_request.id().to_string(),
                        scope: permission_request.scope(),
                    });
                }

                let permission_outcomes = join_all(
                    permission_requests
                        .into_iter()
                        .map(|request| self.permission_policy.decide(request)),
                )
                .await;

                let mut denied_results = Vec::new();
                let mut runnable = Vec::new();
                for (offset, (tool_call, decision)) in
                    batch.iter().zip(permission_outcomes).enumerate()
                {
                    let decision = decision?;
                    if decision.allows_execution() {
                        runnable.push(offset);
                        continue;
                    }

                    let message = decision
                        .message()
                        .unwrap_or("tool execution denied by permission policy")
                        .to_string();
                    events.push(HarnessEvent::PermissionDenied {
                        event_id: EventId::new(),
                        session_id: session_state.session_id().clone(),
                        turn_id: turn_id.clone(),
                        tool_name: tool_call.name().to_string(),
                        request_id: decision.request_id().to_string(),
                        message: message.clone(),
                    });
                    denied_results.push((
                        offset,
                        ToolResult::json(json!({
                            "error": message,
                            "error_kind": RuntimeErrorKind::PermissionDenied,
                        })),
                    ));
                }

                let executable_batch: Vec<_> =
                    runnable.iter().map(|index| &batch[*index]).collect();
                let execution_results = if executable_batch.len() > 1 {
                    for tool_call in &executable_batch {
                        events.push(HarnessEvent::ToolProgress {
                            event_id: EventId::new(),
                            session_id: session_state.session_id().clone(),
                            turn_id: turn_id.clone(),
                            tool_name: tool_call.name().to_string(),
                            phase: "started".to_string(),
                            message: None,
                        });
                    }
                    join_all(executable_batch.iter().map(|tool_call| async {
                        self.tools
                            .execute(tool_call.name(), &self.workspace, tool_call.input().clone())
                            .await
                    }))
                    .await
                } else if let Some(tool_call) = executable_batch.first() {
                    events.push(HarnessEvent::ToolProgress {
                        event_id: EventId::new(),
                        session_id: session_state.session_id().clone(),
                        turn_id: turn_id.clone(),
                        tool_name: tool_call.name().to_string(),
                        phase: "started".to_string(),
                        message: None,
                    });
                    vec![
                        self.tools
                            .execute(tool_call.name(), &self.workspace, tool_call.input().clone())
                            .await,
                    ]
                } else {
                    Vec::new()
                };

                let mut results: Vec<Option<Result<ToolResult, HarnessError>>> =
                    (0..batch.len()).map(|_| None).collect();
                for (offset, result) in denied_results {
                    results[offset] = Some(Ok(result));
                }
                for (offset, result) in runnable.into_iter().zip(execution_results) {
                    results[offset] = Some(result);
                }

                let results: Vec<Result<ToolResult, HarnessError>> = results
                    .into_iter()
                    .map(|result| {
                        result.unwrap_or_else(|| {
                            Ok(ToolResult::json(json!({
                                "error": "tool execution was not scheduled",
                            })))
                        })
                    })
                    .collect();

                for (tool_call, result) in batch.iter().zip(results) {
                    let result = match result {
                        Ok(result) => {
                            events.push(HarnessEvent::ToolCallCompleted {
                                event_id: EventId::new(),
                                session_id: session_state.session_id().clone(),
                                turn_id: turn_id.clone(),
                                tool_name: tool_call.name().to_string(),
                            });
                            result
                        }
                        Err(error) => {
                            let message = error.to_string();
                            events.push(HarnessEvent::ToolCallFailed {
                                event_id: EventId::new(),
                                session_id: session_state.session_id().clone(),
                                turn_id: turn_id.clone(),
                                tool_name: tool_call.name().to_string(),
                                message: message.clone(),
                            });
                            ToolResult::json(json!({ "error": message }))
                        }
                    };

                    session_state.push_tool_result(
                        tool_call.id(),
                        tool_call.name(),
                        result.content().to_string(),
                    );
                    executed_tool_calls.push(ExecutedToolCall {
                        id: tool_call.id().to_string(),
                        name: tool_call.name().to_string(),
                        result,
                    });
                }

                index = end;
            }
        }

        Err(HarnessError::IterationLimit {
            limit: self.max_iterations,
        })
    }
}

#[derive(Default)]
pub struct AgentHarnessBuilder {
    model_client: Option<Arc<dyn ModelClient>>,
    workspace: Option<Workspace>,
    tools: Option<ToolRegistry>,
    permission_policy: Option<Arc<dyn PermissionPolicy>>,
    context_window: Option<ContextWindowState>,
    max_iterations: Option<u32>,
}

impl AgentHarnessBuilder {
    pub fn model_client<T>(mut self, model_client: T) -> Self
    where
        T: ModelClient + 'static,
    {
        self.model_client = Some(Arc::new(model_client));
        self
    }

    pub fn workspace(mut self, workspace: Workspace) -> Self {
        self.workspace = Some(workspace);
        self
    }

    pub fn tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn permission_policy<T>(mut self, permission_policy: T) -> Self
    where
        T: PermissionPolicy + 'static,
    {
        self.permission_policy = Some(Arc::new(permission_policy));
        self
    }

    pub fn context_window(mut self, context_window: ContextWindowState) -> Self {
        self.context_window = Some(context_window);
        self
    }

    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    pub fn build(self) -> Result<AgentHarness, HarnessError> {
        Ok(AgentHarness {
            model_client: self
                .model_client
                .ok_or_else(|| HarnessError::Configuration {
                    message: "model client is required".to_string(),
                })?,
            workspace: self.workspace.ok_or_else(|| HarnessError::Configuration {
                message: "workspace is required".to_string(),
            })?,
            tools: self.tools.unwrap_or_default(),
            permission_policy: self
                .permission_policy
                .unwrap_or_else(|| Arc::new(AllowAllPermissionPolicy)),
            context_window: self.context_window,
            max_iterations: self.max_iterations.unwrap_or(4),
        })
    }
}
