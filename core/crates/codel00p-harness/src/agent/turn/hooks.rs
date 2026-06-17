use super::*;

impl AgentHarness {
    pub(super) async fn run_lifecycle_hook(
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

    pub(super) async fn compact_context_if_needed(
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

    pub(super) async fn run_post_tool_hook(
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

    pub(super) async fn extract_memory_candidates(
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

    /// Auto-recommend memory candidates from the completed turn and queue them
    /// for review. This is the post-session "Memory 2.0" path: even when the
    /// agent emitted no explicit `remember:` directive, a productive turn can
    /// still surface durable facts for a human to approve. Recommendations flow
    /// into the same sink as explicit candidates and are never auto-approved.
    /// A no-op when no recommender is configured.
    pub(super) async fn recommend_memory_candidates(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        goal: String,
        assistant_message: Option<String>,
        tool_calls: Vec<(String, Option<String>)>,
        events: &mut Vec<HarnessEvent>,
    ) {
        let (Some(recommender), Some(sink)) =
            (&self.memory_recommender, &self.memory_candidate_sink)
        else {
            return;
        };

        let candidates = match recommender
            .recommend(TurnMemoryRecommendationRequest::new(
                session_id.clone(),
                turn_id.clone(),
                goal,
                assistant_message,
                tool_calls,
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
                        hook: "memory_recommendation".to_string(),
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

    pub(super) async fn extract_skill_candidates(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        goal: String,
        assistant_message: Option<String>,
        tool_calls: Vec<String>,
        events: &mut Vec<HarnessEvent>,
    ) {
        let (Some(extractor), Some(sink)) = (&self.skill_extractor, &self.skill_proposal_sink)
        else {
            return;
        };

        let proposals = match extractor
            .extract(SkillExtractionRequest::new(
                session_id.clone(),
                turn_id.clone(),
                goal,
                assistant_message,
                tool_calls,
            ))
            .await
        {
            Ok(proposals) => proposals,
            Err(error) => {
                self.record_event(
                    events,
                    HarnessEvent::LifecycleHookFailed {
                        event_id: EventId::new(),
                        session_id,
                        turn_id,
                        hook: "skill_extraction".to_string(),
                        message: error.to_string(),
                    },
                )
                .await;
                return;
            }
        };

        for proposal in proposals {
            // A duplicate proposal is expected (and benign) on repeated tasks;
            // the sink treats it as a no-op, so only genuine errors surface here.
            if let Err(error) = sink.propose(proposal).await {
                self.record_event(
                    events,
                    HarnessEvent::LifecycleHookFailed {
                        event_id: EventId::new(),
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                        hook: "skill_proposal".to_string(),
                        message: error.to_string(),
                    },
                )
                .await;
            }
        }
    }

    /// Auto-extract a capability from the completed turn and queue it for review.
    /// Closes the capability-synthesis loop: a successful pipeline can become a
    /// reusable tool without the agent explicitly calling `propose_capability`.
    pub(super) async fn extract_capability_candidate(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        goal: String,
        assistant_message: Option<String>,
        calls: Vec<crate::capability::CapabilityCandidateCall>,
        events: &mut Vec<HarnessEvent>,
    ) {
        let (Some(extractor), Some(sink)) =
            (&self.capability_extractor, &self.capability_proposal_sink)
        else {
            return;
        };

        let candidate = match extractor
            .extract(crate::capability::CapabilityExtractionRequest {
                goal,
                assistant_message,
                calls,
            })
            .await
        {
            Ok(candidate) => candidate,
            Err(error) => {
                self.record_event(
                    events,
                    HarnessEvent::LifecycleHookFailed {
                        event_id: EventId::new(),
                        session_id,
                        turn_id,
                        hook: "capability_extraction".to_string(),
                        message: error.to_string(),
                    },
                )
                .await;
                return;
            }
        };

        if let Some(capability) = candidate
            && let Err(error) = sink.propose(capability).await
        {
            self.record_event(
                events,
                HarnessEvent::LifecycleHookFailed {
                    event_id: EventId::new(),
                    session_id,
                    turn_id,
                    hook: "capability_proposal".to_string(),
                    message: error.to_string(),
                },
            )
            .await;
        }
    }

    pub(super) async fn record_event(&self, events: &mut Vec<HarnessEvent>, event: HarnessEvent) {
        if let Some(event_sink) = &self.event_sink {
            event_sink.emit(&event).await;
        }
        events.push(event);
    }
}
