use std::sync::Arc;

use futures::future::join_all;

use crate::{
    errors::HarnessError,
    event_sink::AgentEventSink,
    events::HarnessEvent,
    instructions::ProjectInstructionLoader,
    iteration_budget::IterationBudget,
    lifecycle::{LifecycleHook, TurnLifecycleContext},
    memory::{
        MemoryCandidateSink, ProjectMemoryProvider, ProjectMemoryRequest,
        TurnMemoryExtractionRequest, TurnMemoryExtractor,
    },
    permissions::{AllowAllPermissionPolicy, PermissionPolicy, PermissionRequest},
    session::{SessionId, SessionMessage, SessionState, TurnId, UserMessage},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    turn::{ExecutedToolCall, HarnessInferenceRequest, ModelClient, TokenSink, TurnOutcome},
    workspace::Workspace,
};
use codel00p_protocol::{ContextWindowState, EventId, RuntimeErrorKind};
use serde_json::json;

const DEFAULT_COMPACTION_RECENT_MESSAGES: usize = 4;

pub struct AgentHarness {
    model_client: Arc<dyn ModelClient>,
    workspace: Workspace,
    tools: ToolRegistry,
    permission_policy: Arc<dyn PermissionPolicy>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
    lifecycle_hooks: Vec<Arc<dyn LifecycleHook>>,
    project_memory_provider: Option<Arc<dyn ProjectMemoryProvider>>,
    turn_memory_extractor: Option<Arc<dyn TurnMemoryExtractor>>,
    memory_candidate_sink: Option<Arc<dyn MemoryCandidateSink>>,
    context_window: Option<ContextWindowState>,
    token_sink: Option<Arc<dyn TokenSink>>,
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
        self.run_turn_with_state(SessionState::new(session_id), user_message)
            .await
    }

    pub async fn run_turn_with_state(
        self,
        mut session_state: SessionState,
        user_message: UserMessage,
    ) -> Result<TurnOutcome, HarnessError> {
        let turn_id = TurnId::new();
        let mut events = Vec::new();
        self.record_event(
            &mut events,
            HarnessEvent::TurnStarted {
                event_id: EventId::new(),
                session_id: session_state.session_id().clone(),
                turn_id: turn_id.clone(),
            },
        )
        .await;
        session_state.push_user(user_message);
        self.run_lifecycle_hook(
            "turn_started",
            TurnLifecycleContext::new(
                session_state.session_id().clone(),
                turn_id.clone(),
                session_state.messages().len(),
            ),
            &mut events,
        )
        .await;
        self.compact_context_if_needed(&mut session_state, &turn_id, &mut events)
            .await;
        self.record_event(
            &mut events,
            HarnessEvent::ContextBuilt {
                event_id: EventId::new(),
                session_id: session_state.session_id().clone(),
                turn_id: turn_id.clone(),
                message_count: session_state.messages().len(),
            },
        )
        .await;

        let mut executed_tool_calls = Vec::new();

        let budget = IterationBudget::new(self.max_iterations);
        while budget.consume() {
            let iteration = budget.used();
            self.run_lifecycle_hook(
                "pre_inference",
                TurnLifecycleContext::new(
                    session_state.session_id().clone(),
                    turn_id.clone(),
                    session_state.messages().len(),
                ),
                &mut events,
            )
            .await;

            let mut request = HarnessInferenceRequest::new(session_state.clone())
                .with_runtime_context(
                    self.workspace.root().display().to_string(),
                    self.tools.names(),
                );
            if let Some(context_window) = &self.context_window {
                request = request.with_context_window(context_window.clone());
            }
            let project_instructions = ProjectInstructionLoader.load(&self.workspace)?;
            if !project_instructions.is_empty() {
                request = request.with_project_instructions(project_instructions);
            }
            if let Some(project_memory_provider) = &self.project_memory_provider {
                let project_memory = project_memory_provider
                    .retrieve(ProjectMemoryRequest::new(
                        session_state.session_id().clone(),
                        turn_id.clone(),
                        session_state.messages().len(),
                    ))
                    .await?;
                if !project_memory.is_empty() {
                    request = request.with_project_memory(project_memory);
                }
            }

            let response = match &self.token_sink {
                Some(sink) => {
                    self.model_client
                        .infer_streaming(request, sink.as_ref())
                        .await?
                }
                None => self.model_client.infer(request).await?,
            };

            self.record_event(
                &mut events,
                HarnessEvent::InferenceRequested {
                    event_id: EventId::new(),
                    session_id: session_state.session_id().clone(),
                    turn_id: turn_id.clone(),
                    provider: response.provider().to_string(),
                    model: response.model().to_string(),
                },
            )
            .await;
            self.record_event(
                &mut events,
                HarnessEvent::InferenceCompleted {
                    event_id: EventId::new(),
                    session_id: session_state.session_id().clone(),
                    turn_id: turn_id.clone(),
                    finish_reason: response.finish_reason().map(str::to_string),
                },
            )
            .await;

            if response.tool_calls().is_empty() {
                let assistant_message = response.assistant_message().map(str::to_string);
                if let Some(content) = &assistant_message {
                    session_state.push_assistant(content);
                }

                self.extract_memory_candidates(
                    session_state.session_id().clone(),
                    turn_id.clone(),
                    assistant_message.clone(),
                    session_state.messages().len(),
                    &mut events,
                )
                .await;

                self.run_lifecycle_hook(
                    "turn_completed",
                    TurnLifecycleContext::new(
                        session_state.session_id().clone(),
                        turn_id.clone(),
                        session_state.messages().len(),
                    ),
                    &mut events,
                )
                .await;

                self.record_event(
                    &mut events,
                    HarnessEvent::TurnCompleted {
                        event_id: EventId::new(),
                        session_id: session_state.session_id().clone(),
                        turn_id,
                        iterations: iteration,
                    },
                )
                .await;

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
                    self.record_event(
                        &mut events,
                        HarnessEvent::ToolCallRequested {
                            event_id: EventId::new(),
                            session_id: session_state.session_id().clone(),
                            turn_id: turn_id.clone(),
                            tool_name: tool_call.name().to_string(),
                        },
                    )
                    .await;
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
                    self.record_event(
                        &mut events,
                        HarnessEvent::PermissionRequested {
                            event_id: EventId::new(),
                            session_id: session_state.session_id().clone(),
                            turn_id: turn_id.clone(),
                            tool_name: tool_call.name().to_string(),
                            request_id: permission_request.id().to_string(),
                            scope: permission_request.scope(),
                        },
                    )
                    .await;
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
                    self.record_event(
                        &mut events,
                        HarnessEvent::PermissionDenied {
                            event_id: EventId::new(),
                            session_id: session_state.session_id().clone(),
                            turn_id: turn_id.clone(),
                            tool_name: tool_call.name().to_string(),
                            request_id: decision.request_id().to_string(),
                            message: message.clone(),
                        },
                    )
                    .await;
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
                        self.record_event(
                            &mut events,
                            HarnessEvent::ToolProgress {
                                event_id: EventId::new(),
                                session_id: session_state.session_id().clone(),
                                turn_id: turn_id.clone(),
                                tool_name: tool_call.name().to_string(),
                                phase: "started".to_string(),
                                message: None,
                            },
                        )
                        .await;
                    }
                    join_all(executable_batch.iter().map(|tool_call| async {
                        self.tools
                            .execute(tool_call.name(), &self.workspace, tool_call.input().clone())
                            .await
                    }))
                    .await
                } else if let Some(tool_call) = executable_batch.first() {
                    self.record_event(
                        &mut events,
                        HarnessEvent::ToolProgress {
                            event_id: EventId::new(),
                            session_id: session_state.session_id().clone(),
                            turn_id: turn_id.clone(),
                            tool_name: tool_call.name().to_string(),
                            phase: "started".to_string(),
                            message: None,
                        },
                    )
                    .await;
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
                            for progress in result.progress() {
                                self.record_event(
                                    &mut events,
                                    HarnessEvent::ToolProgress {
                                        event_id: EventId::new(),
                                        session_id: session_state.session_id().clone(),
                                        turn_id: turn_id.clone(),
                                        tool_name: tool_call.name().to_string(),
                                        phase: progress.phase().to_string(),
                                        message: progress.message().map(ToString::to_string),
                                    },
                                )
                                .await;
                            }
                            self.record_event(
                                &mut events,
                                HarnessEvent::ToolCallCompleted {
                                    event_id: EventId::new(),
                                    session_id: session_state.session_id().clone(),
                                    turn_id: turn_id.clone(),
                                    tool_name: tool_call.name().to_string(),
                                },
                            )
                            .await;
                            result
                        }
                        Err(error) => {
                            let message = error.to_string();
                            self.record_event(
                                &mut events,
                                HarnessEvent::ToolCallFailed {
                                    event_id: EventId::new(),
                                    session_id: session_state.session_id().clone(),
                                    turn_id: turn_id.clone(),
                                    tool_name: tool_call.name().to_string(),
                                    message: message.clone(),
                                },
                            )
                            .await;
                            ToolResult::json(json!({ "error": message }))
                        }
                    };

                    session_state.push_tool_result(
                        tool_call.id(),
                        tool_call.name(),
                        result.content().to_string(),
                    );
                    self.run_post_tool_hook(
                        TurnLifecycleContext::new(
                            session_state.session_id().clone(),
                            turn_id.clone(),
                            session_state.messages().len(),
                        ),
                        tool_call.name(),
                        &mut events,
                    )
                    .await;
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

    async fn run_lifecycle_hook(
        &self,
        hook_name: &str,
        context: TurnLifecycleContext,
        events: &mut Vec<HarnessEvent>,
    ) {
        for hook in &self.lifecycle_hooks {
            let result = match hook_name {
                "turn_started" => hook.on_turn_started(context.clone()).await,
                "pre_inference" => hook.on_pre_inference(context.clone()).await,
                "pre_compact" => hook.on_pre_compact(context.clone()).await,
                "turn_completed" => hook.on_turn_completed(context.clone()).await,
                _ => Ok(()),
            };
            if let Err(error) = result {
                self.record_event(
                    events,
                    HarnessEvent::LifecycleHookFailed {
                        event_id: EventId::new(),
                        session_id: context.session_id().clone(),
                        turn_id: context.turn_id().clone(),
                        hook: hook_name.to_string(),
                        message: error.to_string(),
                    },
                )
                .await;
            }
        }
    }

    async fn compact_context_if_needed(
        &self,
        session_state: &mut SessionState,
        turn_id: &TurnId,
        events: &mut Vec<HarnessEvent>,
    ) {
        let Some(context_window) = &self.context_window else {
            return;
        };
        if !context_window.is_at_blocking_limit()
            || session_state.messages().len() <= DEFAULT_COMPACTION_RECENT_MESSAGES + 1
        {
            return;
        }

        self.run_lifecycle_hook(
            "pre_compact",
            TurnLifecycleContext::new(
                session_state.session_id().clone(),
                turn_id.clone(),
                session_state.messages().len(),
            ),
            events,
        )
        .await;

        let summary = summarize_compacted_messages(
            session_state.messages(),
            DEFAULT_COMPACTION_RECENT_MESSAGES,
        );
        let record =
            session_state.compact_with_summary(summary.clone(), DEFAULT_COMPACTION_RECENT_MESSAGES);
        self.record_event(
            events,
            HarnessEvent::ContextCompacted {
                event_id: EventId::new(),
                session_id: session_state.session_id().clone(),
                turn_id: turn_id.clone(),
                before_message_count: record.before_message_count(),
                after_message_count: record.after_message_count(),
                summary: Some(summary),
            },
        )
        .await;
    }

    async fn run_post_tool_hook(
        &self,
        context: TurnLifecycleContext,
        tool_name: &str,
        events: &mut Vec<HarnessEvent>,
    ) {
        for hook in &self.lifecycle_hooks {
            if let Err(error) = hook.on_post_tool(context.clone(), tool_name).await {
                self.record_event(
                    events,
                    HarnessEvent::LifecycleHookFailed {
                        event_id: EventId::new(),
                        session_id: context.session_id().clone(),
                        turn_id: context.turn_id().clone(),
                        hook: "post_tool".to_string(),
                        message: error.to_string(),
                    },
                )
                .await;
            }
        }
    }

    async fn extract_memory_candidates(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        assistant_message: Option<String>,
        message_count: usize,
        events: &mut Vec<HarnessEvent>,
    ) {
        let (Some(extractor), Some(sink)) =
            (&self.turn_memory_extractor, &self.memory_candidate_sink)
        else {
            return;
        };

        let candidates = match extractor
            .extract(TurnMemoryExtractionRequest::new(
                session_id.clone(),
                turn_id.clone(),
                assistant_message,
                message_count,
            ))
            .await
        {
            Ok(candidates) => candidates,
            Err(error) => {
                self.record_event(
                    events,
                    HarnessEvent::LifecycleHookFailed {
                        event_id: EventId::new(),
                        session_id,
                        turn_id,
                        hook: "memory_extraction".to_string(),
                        message: error.to_string(),
                    },
                )
                .await;
                return;
            }
        };

        if candidates.is_empty() {
            return;
        }

        if let Err(error) = sink.persist(candidates).await {
            self.record_event(
                events,
                HarnessEvent::LifecycleHookFailed {
                    event_id: EventId::new(),
                    session_id,
                    turn_id,
                    hook: "memory_candidate_sink".to_string(),
                    message: error.to_string(),
                },
            )
            .await;
        }
    }

    async fn record_event(&self, events: &mut Vec<HarnessEvent>, event: HarnessEvent) {
        if let Some(event_sink) = &self.event_sink {
            event_sink.emit(&event).await;
        }
        events.push(event);
    }
}

fn summarize_compacted_messages(messages: &[SessionMessage], keep_recent: usize) -> String {
    let compacted_count = messages.len().saturating_sub(keep_recent);
    let mut lines = vec![format!(
        "Compacted {compacted_count} older session messages."
    )];
    for (index, message) in messages.iter().take(compacted_count).enumerate() {
        let mut content = message.content().replace('\n', " ");
        if content.len() > 200 {
            content.truncate(200);
            content.push_str("...");
        }
        lines.push(format!("{}. {:?}: {}", index + 1, message.role(), content));
    }
    lines.join("\n")
}

#[derive(Default)]
pub struct AgentHarnessBuilder {
    model_client: Option<Arc<dyn ModelClient>>,
    workspace: Option<Workspace>,
    tools: Option<ToolRegistry>,
    permission_policy: Option<Arc<dyn PermissionPolicy>>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
    lifecycle_hooks: Vec<Arc<dyn LifecycleHook>>,
    project_memory_provider: Option<Arc<dyn ProjectMemoryProvider>>,
    turn_memory_extractor: Option<Arc<dyn TurnMemoryExtractor>>,
    memory_candidate_sink: Option<Arc<dyn MemoryCandidateSink>>,
    context_window: Option<ContextWindowState>,
    token_sink: Option<Arc<dyn TokenSink>>,
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

    pub fn event_sink<T>(mut self, event_sink: T) -> Self
    where
        T: AgentEventSink + 'static,
    {
        self.event_sink = Some(Arc::new(event_sink));
        self
    }

    pub fn context_window(mut self, context_window: ContextWindowState) -> Self {
        self.context_window = Some(context_window);
        self
    }

    pub fn lifecycle_hook<T>(mut self, lifecycle_hook: T) -> Self
    where
        T: LifecycleHook + 'static,
    {
        self.lifecycle_hooks.push(Arc::new(lifecycle_hook));
        self
    }

    pub fn project_memory_provider<T>(mut self, project_memory_provider: T) -> Self
    where
        T: ProjectMemoryProvider + 'static,
    {
        self.project_memory_provider = Some(Arc::new(project_memory_provider));
        self
    }

    pub fn turn_memory_extractor<T>(mut self, turn_memory_extractor: T) -> Self
    where
        T: TurnMemoryExtractor + 'static,
    {
        self.turn_memory_extractor = Some(Arc::new(turn_memory_extractor));
        self
    }

    pub fn memory_candidate_sink<T>(mut self, memory_candidate_sink: T) -> Self
    where
        T: MemoryCandidateSink + 'static,
    {
        self.memory_candidate_sink = Some(Arc::new(memory_candidate_sink));
        self
    }

    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    /// Streams assistant text to `token_sink` as it is generated. Without a sink
    /// the harness uses the non-streaming inference path.
    pub fn token_sink<T>(mut self, token_sink: T) -> Self
    where
        T: TokenSink + 'static,
    {
        self.token_sink = Some(Arc::new(token_sink));
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
            event_sink: self.event_sink,
            lifecycle_hooks: self.lifecycle_hooks,
            project_memory_provider: self.project_memory_provider,
            turn_memory_extractor: self.turn_memory_extractor,
            memory_candidate_sink: self.memory_candidate_sink,
            context_window: self.context_window,
            token_sink: self.token_sink,
            max_iterations: self.max_iterations.unwrap_or(4),
        })
    }
}
