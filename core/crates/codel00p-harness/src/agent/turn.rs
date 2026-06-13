//! Agent turn orchestration, tool execution, hooks, and post-turn extraction.

use super::*;

mod hooks;

impl AgentHarness {
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
            if let Some(skill_provider) = &self.skill_provider {
                let skills = skill_provider
                    .select(SkillSelectionRequest::new(
                        session_state.session_id().clone(),
                        turn_id.clone(),
                        session_state.messages().len(),
                        latest_user_message(&session_state),
                    ))
                    .await?;
                if !skills.is_empty() {
                    request = request.with_skills(skills);
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

                self.extract_skill_candidates(
                    session_state.session_id().clone(),
                    turn_id.clone(),
                    latest_user_message(&session_state),
                    assistant_message.clone(),
                    executed_tool_calls
                        .iter()
                        .map(|call: &ExecutedToolCall| call.name.clone())
                        .collect(),
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
}
