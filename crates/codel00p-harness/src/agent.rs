use std::sync::Arc;

use futures::future::join_all;

use crate::{
    errors::HarnessError,
    events::HarnessEvent,
    session::{SessionId, SessionState, TurnId, UserMessage},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    turn::{ExecutedToolCall, HarnessInferenceRequest, ModelClient, TurnOutcome},
    workspace::Workspace,
};
use codel00p_protocol::EventId;
use serde_json::json;

pub struct AgentHarness {
    model_client: Arc<dyn ModelClient>,
    workspace: Workspace,
    tools: ToolRegistry,
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
            let response = self
                .model_client
                .infer(
                    HarnessInferenceRequest::new(session_state.clone()).with_runtime_context(
                        self.workspace.root().display().to_string(),
                        self.tools.names().into_iter().map(str::to_string).collect(),
                    ),
                )
                .await?;

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

                let results = if batch.len() > 1 {
                    join_all(batch.iter().map(|tool_call| async {
                        self.tools
                            .execute(tool_call.name(), &self.workspace, tool_call.input().clone())
                            .await
                    }))
                    .await
                } else {
                    vec![
                        self.tools
                            .execute(batch[0].name(), &self.workspace, batch[0].input().clone())
                            .await,
                    ]
                };

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
            max_iterations: self.max_iterations.unwrap_or(4),
        })
    }
}
