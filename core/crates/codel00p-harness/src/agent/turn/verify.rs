//! Verify-before-done loop and self-critique (metacognition) for the turn loop.
//!
//! When the model signals it is done (no tool calls) at the end of a turn that
//! made **mutating** changes, the harness runs a verification phase BEFORE
//! actually completing the turn:
//!
//! 1. **Verify** — run the project's checks through the harness's own registered
//!    `run_checks` tool (reusing detection, the terminal backend, truncation,
//!    summary parsing, and permissions). On failure, feed the captured failure
//!    back into the conversation and keep looping (bounded by
//!    `verify_iterations`) so the model fixes it and re-verifies. On pass — or
//!    when there is nothing to verify — proceed.
//! 2. **Self-critique** — give the model one reflection turn to review what it
//!    changed and what it actually verified, addressing or plainly stating any
//!    gap. If it then calls tools the loop continues; otherwise the turn
//!    completes.
//!
//! This directly addresses the dogfooding failure where unit tests were green
//! while the running app was broken: success is no longer self-declared.

use super::*;

/// Mutating tools whose execution means the workspace changed this turn, so a
/// verification pass is warranted. A turn that ran none of these is read-only
/// and verification is skipped (nothing to verify).
const MUTATING_TOOLS: &[&str] = &[
    "create_file",
    "update_file",
    "delete_file",
    "apply_patch",
    "run_command",
];

/// What the done-point should do after the verify/self-critique phase.
pub(super) enum DonePhase {
    /// Finish the turn now. Carries the verification verdict for `TurnCompleted`
    /// (`None` = verification did not apply; `Some(true/false)` = ran + verdict).
    Complete { verified: Option<bool> },
    /// Do not finish: a message was appended to the session (a verification
    /// failure to fix, or a self-critique reflection prompt). The loop should run
    /// another inference.
    Continue,
}

/// Per-turn mutable state for the verify-before-done phase, threaded through the
/// loop so the bound and the once-only self-critique are respected.
pub(super) struct VerifyState {
    /// Number of verify→fix attempts consumed.
    pub attempts: u32,
    /// Whether the self-critique reflection step has already been injected.
    pub critique_injected: bool,
    /// Set while we have injected a self-critique prompt and are waiting to see
    /// whether the model's next response produces tool calls; used to emit the
    /// `SelfCritiqueCompleted` marker exactly once with the right verdict.
    pub critique_pending: bool,
    /// Count of executed tool calls at the moment the self-critique prompt was
    /// injected. A later done-point with no additional executed calls means the
    /// reflection turn was text-only (the final step) and the turn completes; a
    /// higher count means the reflection produced more work that warrants
    /// re-verification.
    pub executed_at_critique: usize,
    /// Last verification verdict, surfaced on `TurnCompleted`.
    pub last_verified: Option<bool>,
}

impl VerifyState {
    pub(super) fn new() -> Self {
        Self {
            attempts: 0,
            critique_injected: false,
            critique_pending: false,
            executed_at_critique: 0,
            last_verified: None,
        }
    }
}

impl AgentHarness {
    /// Whether any executed tool call this turn actually mutated the workspace.
    ///
    /// A call counts only when it is a known mutating tool AND it actually ran to
    /// success — a denied or failed attempt (its result carries an `error` key)
    /// changed nothing, so it does not warrant a verification pass.
    pub(super) fn turn_mutated(executed: &[ExecutedToolCall]) -> bool {
        executed.iter().any(|call| {
            MUTATING_TOOLS.contains(&call.name.as_str())
                && call.result.content().get("error").is_none()
        })
    }

    /// The done-point decision: run the verify-before-done loop and the
    /// self-critique step, returning whether to complete or keep looping.
    ///
    /// Called once per "model signalled done" event. Mutates `session_state`
    /// (appending failure feedback or a reflection prompt) and `verify_state`.
    pub(super) async fn verify_before_done(
        &self,
        session_state: &mut SessionState,
        turn_id: &TurnId,
        executed_tool_calls: &[ExecutedToolCall],
        verify_state: &mut VerifyState,
        events: &mut Vec<HarnessEvent>,
    ) -> DonePhase {
        // After the self-critique reflection turn, only re-verify if the
        // reflection produced *new* mutating work. A text-only reflection (no
        // additional executed calls since it was injected) is the final step —
        // verification already passed before it, so complete without re-running.
        let reflection_added_work = !verify_state.critique_injected
            || executed_tool_calls.len() > verify_state.executed_at_critique;

        // Phase 1: verify-before-done.
        if self.verify.self_verify
            && reflection_added_work
            && Self::turn_mutated(executed_tool_calls)
            && verify_state.attempts < self.verify.verify_iterations
        {
            match self
                .run_verification(session_state, turn_id, verify_state, events)
                .await
            {
                VerificationOutcome::Failed => return DonePhase::Continue,
                VerificationOutcome::Passed => { /* fall through to self-critique */ }
                VerificationOutcome::Skipped => { /* nothing to verify */ }
            }
        }

        // Phase 2: self-critique (one reflection turn, once per turn). Gated on
        // the turn having mutated the workspace — reflecting on "what you changed
        // and verified" is only meaningful when the agent actually did work, so a
        // read-only Q&A turn stays a single inference (and incurs no extra cost).
        if self.verify.self_critique
            && !verify_state.critique_injected
            && Self::turn_mutated(executed_tool_calls)
        {
            verify_state.critique_injected = true;
            verify_state.critique_pending = true;
            verify_state.executed_at_critique = executed_tool_calls.len();
            session_state.push_message(SessionMessage::user(SELF_CRITIQUE_PROMPT));
            return DonePhase::Continue;
        }

        DonePhase::Complete {
            verified: verify_state.last_verified,
        }
    }

    /// Run the configured checks (`test`, plus `lint` when `lint_and_fix`)
    /// through the registered `run_checks` tool. Appends failure feedback to the
    /// session on the first failing check and returns the outcome.
    async fn run_verification(
        &self,
        session_state: &mut SessionState,
        turn_id: &TurnId,
        verify_state: &mut VerifyState,
        events: &mut Vec<HarnessEvent>,
    ) -> VerificationOutcome {
        // Graceful skip: no `run_checks` tool registered (command tools disabled).
        if !self.tools.names().iter().any(|name| name == "run_checks") {
            return VerificationOutcome::Skipped;
        }
        // Graceful skip: no detectable test command and no explicit override.
        let detected = crate::checks::detect_checks(&self.workspace);
        if detected.test.is_none() && self.verify.test_command.is_none() {
            return VerificationOutcome::Skipped;
        }

        let mut checks: Vec<&str> = Vec::new();
        if self.verify.auto_test {
            checks.push("test");
        }
        if self.verify.lint_and_fix {
            checks.push("lint");
        }
        if checks.is_empty() {
            return VerificationOutcome::Skipped;
        }

        verify_state.attempts += 1;
        let attempt = verify_state.attempts;

        for check in checks {
            let mut input = json!({ "check": check });
            // The explicit override applies to the `test` check only; lint keeps
            // detection so an opt-in lint pass uses the project's linter.
            if check == "test"
                && let Some(command) = &self.verify.test_command
            {
                input["command"] = json!(command);
            }

            let result = self
                .tools
                .execute("run_checks", &self.workspace, input)
                .await;

            let content = match result {
                Ok(result) => result.content().clone(),
                Err(error) => {
                    // The check could not even run (e.g. detection error). Treat
                    // as a non-fatal skip for this check rather than blocking the
                    // turn forever — surface it and move on.
                    self.record_event(
                        events,
                        HarnessEvent::ToolCallFailed {
                            event_id: EventId::new(),
                            session_id: session_state.session_id().clone(),
                            turn_id: turn_id.clone(),
                            tool_name: "run_checks".to_string(),
                            message: error.to_string(),
                        },
                    )
                    .await;
                    continue;
                }
            };

            let success = content
                .get("success")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let command = content
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(check)
                .to_string();
            let exit_code = content.get("exit_code").and_then(serde_json::Value::as_i64);

            self.record_event(
                events,
                HarnessEvent::VerificationCompleted {
                    event_id: EventId::new(),
                    session_id: session_state.session_id().clone(),
                    turn_id: turn_id.clone(),
                    check: check.to_string(),
                    command: command.clone(),
                    success,
                    exit_code,
                    attempt,
                },
            )
            .await;

            if !success {
                verify_state.last_verified = Some(false);
                let failures = capture_failures(&content);
                let budget_note = if attempt >= self.verify.verify_iterations {
                    "\n\nThis was the final automated verification attempt; if it \
                     still fails the turn will end with verification UNRESOLVED."
                } else {
                    ""
                };
                session_state.push_message(SessionMessage::user(format!(
                    "Automated verification failed (`{command}` exited {code}):\n{failures}\n\n\
                     Fix the issue and continue; do not finish until it passes.{budget_note}",
                    code = exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "non-zero".to_string()),
                )));
                return VerificationOutcome::Failed;
            }
        }

        verify_state.last_verified = Some(true);
        VerificationOutcome::Passed
    }
}

/// Result of running the configured checks once.
enum VerificationOutcome {
    /// All configured checks passed.
    Passed,
    /// A check failed; failure feedback was appended to the session.
    Failed,
    /// Nothing to verify (no `run_checks` tool, no detectable command, or no
    /// checks enabled) — verification is silently skipped.
    Skipped,
}

/// The single reflection instruction injected for the self-critique step.
const SELF_CRITIQUE_PROMPT: &str = "\
Before finishing, review your own work: what did you actually change, and what \
did you actually verify? If anything is unverified, risky, or you over-claimed \
that something works, address it now (make the fix and/or run the relevant \
checks). If everything is genuinely done and verified, briefly confirm what you \
verified. Do not claim success you have not checked.";

/// Build a compact, model-facing failure excerpt from a `run_checks` result:
/// the parsed pass/fail summary (when present) plus captured stderr/stdout.
fn capture_failures(content: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    if let Some(summary) = content.get("summary").filter(|s| !s.is_null()) {
        let passed = summary.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
        let failed = summary.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
        parts.push(format!("summary: {passed} passed, {failed} failed"));
    }
    let stderr = content.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
    let stdout = content.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let body = if !stderr.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    if !body.trim().is_empty() {
        parts.push(body.trim().to_string());
    }
    if parts.is_empty() {
        "(no captured output)".to_string()
    } else {
        parts.join("\n")
    }
}
