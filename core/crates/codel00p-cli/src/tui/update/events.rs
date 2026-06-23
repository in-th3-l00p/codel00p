//! Folding harness events and turn completion into `App` state.
//!
//! `apply_event` maps each streamed [`HarnessEvent`] onto the transcript and the
//! status bar (tool lifecycle, usage/cost, verification, self-critique, replan
//! signals); `handle_turn_finished` settles the turn — finalizing the assistant
//! message, adopting the new session state, and emitting the persist effect.

use super::*;

pub(super) fn apply_event(app: &mut App, event: &HarnessEvent) {
    use codel00p_protocol::AgentEvent::*;
    match event {
        ToolCallRequested { tool_name, .. } => {
            app.conversation.tool_requested(tool_name);
            app.turn.current_tool = Some(tool_name.clone());
        }
        ToolProgress {
            tool_name, message, ..
        } => app.conversation.tool_progress(tool_name, message.clone()),
        ToolCallCompleted { tool_name, .. } => {
            app.conversation.tool_completed(tool_name);
            app.turn.current_tool = None;
        }
        ToolCallFailed {
            tool_name, message, ..
        } => {
            app.conversation.tool_failed(tool_name, message);
            app.turn.current_tool = None;
        }
        PermissionDenied {
            tool_name, message, ..
        } => app
            .conversation
            .push_notice(format!("Permission denied for {tool_name}: {message}")),
        ContextCompacted {
            before_message_count,
            after_message_count,
            ..
        } => app.conversation.push_notice(format!(
            "Context compacted: {before_message_count} → {after_message_count} messages."
        )),
        InferenceCompleted {
            finish_reason,
            usage,
            cost,
            ..
        } => {
            app.turn.finish_reason = finish_reason.clone();
            // Capture real provider token usage when reported; this is preferred
            // over the char-count estimate in the advanced status bar.
            if let Some(usage) = usage {
                app.last_usage = Some(usage.clone());
            }
            // Capture the cost estimate when the provider priced the call. Left
            // untouched when absent so the HUD never shows a bogus `$0.00`.
            if let Some(cost) = cost {
                app.last_cost = Some(cost.clone());
            }
        }
        TurnCompleted {
            iterations,
            usage,
            cost,
            ..
        } => {
            app.turn.iterations = *iterations;
            // The turn-total usage supersedes the last per-inference figure.
            if let Some(usage) = usage {
                app.last_usage = Some(usage.clone());
            }
            if let Some(cost) = cost {
                app.last_cost = Some(cost.clone());
            }
        }
        // The verify-before-done loop ran the project's checks and reached a
        // verdict — surface it as a transcript line and a persistent status-bar
        // signal so automated verification is visible (a trust signal).
        VerificationCompleted {
            check,
            command,
            success,
            ..
        } => {
            if *success {
                let line = format!("✓ Verified: {check} pass");
                app.verification = Some(line.clone());
                app.conversation.push_notice(line);
            } else {
                let line = format!("⚠ Verification failed: `{command}` — retrying");
                app.verification = Some(format!("⚠ {check} failed — retrying"));
                app.conversation.push_notice(line);
            }
        }
        // The self-critique reflection step ran. If it produced more tool calls
        // the loop continued (a gap was addressed); otherwise it confirmed done.
        SelfCritiqueCompleted {
            produced_tool_calls,
            ..
        } => {
            let line = if *produced_tool_calls {
                "○ Self-check found a gap — addressing it".to_string()
            } else {
                "○ Self-check done".to_string()
            };
            app.verification = Some(line.clone());
            app.conversation.push_notice(line);
        }
        // The in-turn failure budget was hit: the same operation kept failing and
        // a step-back/replan nudge was injected. Surface it as a visible signal.
        FailureBudgetExceeded {
            operation,
            attempts,
            ..
        } => {
            let line = format!("⚠ repeated failures ({operation} ×{attempts}) — replanning");
            app.verification = Some("⚠ repeated failures — replanning".to_string());
            app.conversation.push_notice(line);
        }
        // `ToolCallArgsDelta` reaches the bridge here (via `ChannelEventSink`)
        // but is intentionally not rendered into the transcript: it is a live
        // signal, and the assembled call is surfaced by `ToolCallRequested`
        // below it. Other consumers (e.g. `--stream-events`) observe the delta.
        _ => {}
    }
}

pub(super) fn handle_turn_finished(
    app: &mut App,
    result: Result<Box<codel00p_harness::TurnOutcome>, String>,
) -> Vec<Effect> {
    app.turn.running = false;
    app.turn.current_tool = None;
    match result {
        Ok(outcome) => {
            if let Some(message) = &outcome.assistant_message {
                app.conversation.finalize_assistant(message);
            }
            let start = app.persisted_message_count;
            app.session_state = outcome.session_state.clone();
            app.persisted_message_count = outcome.session_state.messages().len();
            app.refresh_usage();
            vec![Effect::Persist(outcome, start)]
        }
        Err(message) => {
            app.conversation.push_error(message);
            Vec::new()
        }
    }
}
