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

        // Accumulate token usage / cost across every inference in this turn so
        // the closing `TurnCompleted` can report a turn total. Stays `None`
        // until at least one inference reports usage, preserving the legacy
        // (no-usage) shape for providers that don't surface it.
        let mut turn_usage: Option<TokenUsage> = None;
        let mut turn_cost: Option<CostEstimate> = None;

        let budget = IterationBudget::new(self.max_iterations);
        while budget.consume() {
            let iteration = budget.used();
            // Cooperative cancellation: stop at the iteration boundary (before any
            // new inference) and return what we have so the session still persists.
            if self.cancel.is_cancelled() {
                return self
                    .finish_cancelled(
                        session_state,
                        turn_id,
                        executed_tool_calls,
                        events,
                        iteration.saturating_sub(1),
                        turn_usage,
                        turn_cost,
                    )
                    .await;
            }
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
                    self.tools.advertised_specs(),
                );
            if let Some(tool_choice) = &self.tool_choice {
                request = request.with_tool_choice(tool_choice.clone());
            }
            if let Some(response_format) = &self.response_format {
                request = request.with_response_format(response_format.clone());
            }
            if let Some(context_window) = &self.context_window {
                request = request.with_context_window(context_window.clone());
            }
            // Self-awareness: refresh the live run-state from the sources in scope
            // here (iteration budget, accumulated usage, context window, plan) and
            // render the compact "self" block. Updating the shared handle also
            // makes the latest state visible to the `self_describe` tool.
            if let Some(handle) = &self.agent_self {
                let plan = self.plan_store.as_ref().map(|store| store.current());
                let (plan_completed, plan_total) = match &plan {
                    Some(items) if !items.is_empty() => (
                        Some(
                            items
                                .iter()
                                .filter(|item| {
                                    item.status == crate::planning::PlanStatus::Completed
                                })
                                .count(),
                        ),
                        Some(items.len()),
                    ),
                    _ => (None, None),
                };
                let state = AgentSelfState {
                    iteration: Some(iteration),
                    max_iterations: Some(budget.max_total()),
                    context_used_tokens: turn_usage.as_ref().map(|usage| {
                        usage.input_tokens
                            + usage.output_tokens
                            + usage.cache_read_tokens
                            + usage.cache_write_tokens
                    }),
                    context_window_tokens: self
                        .context_window
                        .as_ref()
                        .map(|window| window.context_limit_tokens()),
                    plan_completed,
                    plan_total,
                };
                handle.set_state(state.clone());
                if let Some(block) = SelfPromptAssembler.assemble(handle.context(), &state) {
                    request = request.with_agent_self(block);
                }
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

            // Emit a deterministic context manifest capturing exactly what
            // went into this inference request — instruction files, injected
            // memory ids, advertised tools, and selected skills.  One event
            // per inference build so the manifest always reflects the actual
            // inputs of the call that follows it.
            {
                let instruction_sources = request
                    .project_instructions()
                    .map(|pi| pi.sources().iter().map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                let injected_memory_ids = request
                    .project_memory()
                    .map(|pm| {
                        pm.items()
                            .iter()
                            .map(|item| item.id().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let advertised_tools = request.tool_names().iter().map(|n| n.to_string()).collect();
                let skill_names = request
                    .skills()
                    .map(|sc| sc.skills().iter().map(|s| s.name().to_string()).collect())
                    .unwrap_or_default();
                let message_count = request.session_state().messages().len();
                self.record_event(
                    &mut events,
                    HarnessEvent::context_manifest(
                        EventId::new(),
                        session_state.session_id().clone(),
                        turn_id.clone(),
                        instruction_sources,
                        injected_memory_ids,
                        advertised_tools,
                        skill_names,
                        message_count,
                    ),
                )
                .await;
            }

            let response = match &self.token_sink {
                Some(sink) => {
                    // Stream assistant text to the caller's sink, and fan
                    // tool-call argument deltas out both to that sink and (when
                    // an event sink is wired) onto the event stream as live
                    // `ToolCallArgsDelta` events. The forwarder drains buffered
                    // deltas concurrently with the inference call; the final
                    // assembled response is unchanged.
                    let (streaming_sink, forwarder) = crate::streaming::StreamingSink::new(
                        Some(sink.clone()),
                        self.event_sink.clone(),
                        session_state.session_id().clone(),
                        turn_id.clone(),
                    );
                    // Drop the sink once inference finishes so the forwarder's
                    // channel closes and `forward` returns after draining.
                    let (response, ()) = futures::join!(
                        async {
                            let response = self
                                .model_client
                                .infer_streaming(request, &streaming_sink)
                                .await;
                            drop(streaming_sink);
                            response
                        },
                        forwarder.forward(),
                    );
                    response?
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
            // Surface per-inference usage/cost on the event, and fold it into
            // the running turn total emitted later on `TurnCompleted`.
            let inference_usage = response.usage().cloned();
            let inference_cost = response.cost().cloned();
            if let Some(usage) = &inference_usage {
                turn_usage
                    .get_or_insert_with(TokenUsage::default)
                    .add(usage);
            }
            if let Some(cost) = &inference_cost {
                turn_cost
                    .get_or_insert_with(CostEstimate::default)
                    .add(cost);
            }
            self.record_event(
                &mut events,
                HarnessEvent::InferenceCompleted {
                    event_id: EventId::new(),
                    session_id: session_state.session_id().clone(),
                    turn_id: turn_id.clone(),
                    finish_reason: response.finish_reason().map(str::to_string),
                    usage: inference_usage,
                    cost: inference_cost,
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

                // Auto-recommend memory candidates from the turn's work, so
                // explicit `remember:` directives and machine recommendations
                // both land in the same review queue.
                self.recommend_memory_candidates(
                    session_state.session_id().clone(),
                    turn_id.clone(),
                    latest_user_message(&session_state),
                    assistant_message.clone(),
                    executed_tool_calls
                        .iter()
                        .map(|call: &ExecutedToolCall| (call.name.clone(), None))
                        .collect(),
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

                self.extract_capability_candidate(
                    session_state.session_id().clone(),
                    turn_id.clone(),
                    latest_user_message(&session_state),
                    assistant_message.clone(),
                    executed_tool_calls
                        .iter()
                        .map(
                            |call: &ExecutedToolCall| crate::capability::CapabilityCandidateCall {
                                name: call.name.clone(),
                                input: call.input.clone(),
                                output: call.result.content().clone(),
                            },
                        )
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
                        usage: turn_usage,
                        cost: turn_cost,
                    },
                )
                .await;

                return Ok(TurnOutcome {
                    assistant_message,
                    tool_calls: executed_tool_calls,
                    events,
                    session_state,
                    cancelled: false,
                });
            }

            // The model wants to run tools. If cancellation was requested while we
            // waited on inference, stop before mutating state with tool calls we
            // will not execute, so a resume sees a consistent transcript.
            if self.cancel.is_cancelled() {
                return self
                    .finish_cancelled(
                        session_state,
                        turn_id,
                        executed_tool_calls,
                        events,
                        iteration,
                        turn_usage,
                        turn_cost,
                    )
                    .await;
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

                    // Cap the result shown to the model so verbose output cannot
                    // flood the context window; the full output stays in the
                    // executed-call record below and (when truncated) on disk.
                    let recorded = self
                        .tool_output_truncation
                        .apply(tool_call.name(), &result.content().to_string());
                    session_state.push_tool_result(tool_call.id(), tool_call.name(), recorded);
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
                        input: tool_call.input().clone(),
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

    /// Finalizes a turn that stopped early because cancellation was requested.
    /// Runs the `turn_completed` hook and emits `TurnCompleted` (so listeners see
    /// the turn end like any other), then returns a `cancelled` outcome carrying
    /// the messages and tool results gathered so far.
    #[allow(clippy::too_many_arguments)]
    async fn finish_cancelled(
        &self,
        session_state: SessionState,
        turn_id: TurnId,
        executed_tool_calls: Vec<ExecutedToolCall>,
        mut events: Vec<HarnessEvent>,
        iterations: u32,
        usage: Option<TokenUsage>,
        cost: Option<CostEstimate>,
    ) -> Result<TurnOutcome, HarnessError> {
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
                iterations,
                usage,
                cost,
            },
        )
        .await;

        Ok(TurnOutcome {
            assistant_message: None,
            tool_calls: executed_tool_calls,
            events,
            session_state,
            cancelled: true,
        })
    }
}
