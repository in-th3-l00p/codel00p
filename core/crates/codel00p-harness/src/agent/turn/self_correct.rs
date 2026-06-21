//! In-turn error self-correction: failure classification, the repeated-failure
//! budget, and the step-back/replan nudge (#12 T0.4).
//!
//! This is about **tool-call** failures *during* a turn — distinct from the
//! end-of-turn verify-before-done phase (see `verify.rs`). When a tool call
//! fails the harness:
//!
//! 1. classifies the failure message (when `error_hints` is on) and enriches the
//!    error payload fed back to the model with `error_kind` + an actionable
//!    `hint`; and
//! 2. tracks consecutive failures of the *same operation* (tool name + the
//!    program/args for command tools, else the tool name) within the turn. When
//!    one operation fails `failure_budget` times in a row (and
//!    `replan_on_failure` is on), it injects a stronger "stop repeating it —
//!    step back / replan" nudge as a user message. The nudge never aborts the
//!    turn; the iteration budget still bounds the overall run.

use std::collections::HashMap;

use serde_json::Value;

use crate::error_classify::classify;

/// Per-turn tracker of consecutive same-operation failures. A signature's count
/// increments each time that operation fails and resets to zero the moment it
/// succeeds, so only *consecutive* failures count toward the budget.
#[derive(Default)]
pub(super) struct FailureTracker {
    counts: HashMap<String, u32>,
}

impl FailureTracker {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Record a successful run of `signature`, clearing any failure streak.
    pub(super) fn record_success(&mut self, signature: &str) {
        self.counts.remove(signature);
    }

    /// Record a failed run of `signature` and return the new consecutive-failure
    /// count for that operation.
    pub(super) fn record_failure(&mut self, signature: &str) -> u32 {
        let entry = self.counts.entry(signature.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }
}

/// The operation signature used to group "the same" tool call across a turn.
///
/// For command-style tools (`run_command`, `run_checks`) the signature is the
/// tool name plus the program and args (or the explicit `command`/`check`), so
/// re-running the *same* command counts toward the budget but a different command
/// starts fresh. For every other tool the signature is just the tool name —
/// simple and robust (repeatedly failing `create_file` calls, even on different
/// paths, still trip the budget, which is the desired "stop flailing" behavior).
pub(super) fn operation_signature(tool_name: &str, input: &Value) -> String {
    match tool_name {
        "run_command" => {
            let program = input.get("program").and_then(Value::as_str).unwrap_or("");
            let args = input
                .get("args")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!("run_command:{program} {args}")
                .trim_end()
                .to_string()
        }
        "run_checks" => {
            let check = input.get("check").and_then(Value::as_str).unwrap_or("");
            let command = input.get("command").and_then(Value::as_str).unwrap_or("");
            format!("run_checks:{check}:{command}")
        }
        other => other.to_string(),
    }
}

/// Whether a (successfully returned) tool result represents a failure the model
/// should self-correct on: an explicit `error` key, or a command result that
/// reported `success: false` (non-zero exit / timeout). A returned `Err` from
/// `execute` is always a failure and handled by the caller before this.
pub(super) fn result_is_failure(content: &Value) -> bool {
    if content.get("error").is_some() {
        return true;
    }
    matches!(content.get("success"), Some(Value::Bool(false)))
}

/// Extract the most informative failure text from a result payload for
/// classification: the `error` string if present, else `stderr`, else `stdout`,
/// else the whole content rendered. Used to feed [`classify`].
pub(super) fn failure_message(content: &Value) -> String {
    if let Some(error) = content.get("error").and_then(Value::as_str) {
        return error.to_string();
    }
    let stderr = content.get("stderr").and_then(Value::as_str).unwrap_or("");
    if !stderr.trim().is_empty() {
        return stderr.to_string();
    }
    if matches!(content.get("timed_out"), Some(Value::Bool(true))) {
        return "operation timed out".to_string();
    }
    let stdout = content.get("stdout").and_then(Value::as_str).unwrap_or("");
    if !stdout.trim().is_empty() {
        return stdout.to_string();
    }
    content.to_string()
}

/// Enrich a failure payload in place with `error_kind` + `hint` from the
/// classifier. Non-destructive: existing keys are preserved, and an already
/// present `error_kind`/`hint` (e.g. permission-denied set upstream) is left
/// alone. No-op for an `unknown` classification that yields no hint.
pub(super) fn enrich_failure(content: &mut Value, message: &str) {
    let Value::Object(map) = content else {
        return;
    };
    let (kind, hint) = classify(message);
    map.entry("error_kind")
        .or_insert_with(|| Value::String(kind.as_str().to_string()));
    if let Some(hint) = hint {
        map.entry("hint")
            .or_insert_with(|| Value::String(hint.to_string()));
    }
}

/// The step-back/replan nudge injected as a user message when an operation
/// exhausts the failure budget. `replan` controls whether the explicit
/// revisit-the-plan sentence is appended (only meaningful when planning is on).
pub(super) fn replan_nudge(
    operation: &str,
    attempts: u32,
    last_error: &str,
    replan: bool,
) -> String {
    let trimmed = last_error.trim();
    let excerpt = if trimmed.len() > 400 {
        format!("{}…", &trimmed[..400])
    } else {
        trimmed.to_string()
    };
    let mut nudge = format!(
        "You have attempted `{operation}` and it has failed {attempts} times in a row with the \
         same kind of error:\n{excerpt}\n\nStop repeating this exact operation — retrying it the \
         same way will keep failing. Step back and reconsider your approach: try a different \
         command/tool, address the underlying cause, or (if you are blocked) ask the user."
    );
    if replan {
        nudge.push_str(
            " If you are working from a plan, revisit and update it (via the planning tool) to \
             reflect this dead end before continuing.",
        );
    }
    nudge
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn run_command_signature_includes_program_and_args() {
        let sig = operation_signature(
            "run_command",
            &json!({ "program": "cargo", "args": ["build", "--workspace"] }),
        );
        assert_eq!(sig, "run_command:cargo build --workspace");
        // A different command is a different signature.
        let other = operation_signature("run_command", &json!({ "program": "ls", "args": [] }));
        assert_ne!(sig, other);
    }

    #[test]
    fn non_command_signature_is_tool_name() {
        assert_eq!(
            operation_signature("create_file", &json!({ "path": "a.rs" })),
            "create_file"
        );
    }

    #[test]
    fn tracker_counts_consecutive_and_resets_on_success() {
        let mut tracker = FailureTracker::new();
        assert_eq!(tracker.record_failure("op"), 1);
        assert_eq!(tracker.record_failure("op"), 2);
        tracker.record_success("op");
        assert_eq!(tracker.record_failure("op"), 1);
    }

    #[test]
    fn detects_failure_from_error_key_and_success_false() {
        assert!(result_is_failure(&json!({ "error": "boom" })));
        assert!(result_is_failure(
            &json!({ "success": false, "exit_code": 1 })
        ));
        assert!(!result_is_failure(&json!({ "success": true })));
        assert!(!result_is_failure(&json!({ "ok": 1 })));
    }

    #[test]
    fn failure_message_prefers_error_then_stderr() {
        assert_eq!(failure_message(&json!({ "error": "nope" })), "nope");
        assert_eq!(
            failure_message(&json!({ "success": false, "stderr": "command not found" })),
            "command not found"
        );
    }

    #[test]
    fn enrich_adds_kind_and_hint_without_clobbering() {
        let mut content = json!({ "error": "cargo: command not found" });
        enrich_failure(&mut content, "cargo: command not found");
        assert_eq!(content["error_kind"], "missing_dependency");
        assert!(content["hint"].as_str().unwrap().contains("dependency"));

        // Preset error_kind is preserved.
        let mut preset = json!({ "error": "denied", "error_kind": "permission_denied" });
        enrich_failure(&mut preset, "permission denied");
        assert_eq!(preset["error_kind"], "permission_denied");
    }

    #[test]
    fn replan_nudge_mentions_operation_and_count() {
        let nudge = replan_nudge("run_command:cargo build", 3, "error[E0425]", true);
        assert!(nudge.contains("run_command:cargo build"));
        assert!(nudge.contains("3 times"));
        assert!(nudge.contains("planning tool"));
        let no_replan = replan_nudge("op", 3, "err", false);
        assert!(!no_replan.contains("planning tool"));
    }
}
