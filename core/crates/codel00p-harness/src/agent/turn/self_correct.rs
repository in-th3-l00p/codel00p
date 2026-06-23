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

use crate::error_classify::{ToolErrorKind, classify};

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

    /// The current consecutive-failure streak for `signature` (zero if none).
    pub(super) fn streak(&self, signature: &str) -> u32 {
        self.counts.get(signature).copied().unwrap_or(0)
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
///
/// `schema` is the failing tool's input schema, when known. On an
/// invalid-input failure it is echoed back as `expected_schema` (size-bounded)
/// so the model can match the exact shape instead of guessing — the highest-
/// leverage recovery lever for a mis-shaped tool call.
pub(super) fn enrich_failure(content: &mut Value, message: &str, schema: Option<&Value>) {
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
    if kind == ToolErrorKind::InvalidInput
        && let Some(schema) = schema
    {
        map.entry("expected_schema")
            .or_insert_with(|| compact_schema(schema));
    }
}

/// Bound the size of an echoed schema so a pathologically large (often MCP) tool
/// schema cannot blow up the context on every retry. Small schemas pass through
/// verbatim; oversized ones are trimmed to the essentials the model needs to fix
/// a call — the top-level `type`/`required` plus each property's declared type.
fn compact_schema(schema: &Value) -> Value {
    const MAX_BYTES: usize = 2_000;
    if schema.to_string().len() <= MAX_BYTES {
        return schema.clone();
    }
    let mut compact = serde_json::Map::new();
    if let Some(kind) = schema.get("type") {
        compact.insert("type".to_string(), kind.clone());
    }
    if let Some(required) = schema.get("required") {
        compact.insert("required".to_string(), required.clone());
    }
    if let Some(Value::Object(props)) = schema.get("properties") {
        let summarized: serde_json::Map<String, Value> = props
            .iter()
            .map(|(name, spec)| {
                let kind = spec
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("any")
                    .to_string();
                (name.clone(), Value::String(kind))
            })
            .collect();
        compact.insert("properties".to_string(), Value::Object(summarized));
    }
    Value::Object(compact)
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
        enrich_failure(&mut content, "cargo: command not found", None);
        assert_eq!(content["error_kind"], "missing_dependency");
        assert!(content["hint"].as_str().unwrap().contains("dependency"));
        // A non-input failure never carries a schema echo.
        assert!(content.get("expected_schema").is_none());

        // Preset error_kind is preserved.
        let mut preset = json!({ "error": "denied", "error_kind": "permission_denied" });
        enrich_failure(&mut preset, "permission denied", None);
        assert_eq!(preset["error_kind"], "permission_denied");
    }

    #[test]
    fn invalid_input_failure_echoes_expected_schema() {
        let schema = json!({
            "type": "object",
            "properties": { "changes": { "items": { "type": "object" } } }
        });
        let message = "invalid input for tool apply_patch: missing string field `path`";
        let mut content = json!({ "error": message });
        enrich_failure(&mut content, message, Some(&schema));
        assert_eq!(content["error_kind"], "invalid_input");
        // The exact schema the model needs to match is attached.
        assert_eq!(content["expected_schema"], schema);
        assert!(
            content["hint"]
                .as_str()
                .unwrap()
                .contains("expected_schema")
        );
    }

    #[test]
    fn schema_echo_only_on_invalid_input() {
        // A schema is available, but the failure is not an input error, so it is
        // not echoed (no needless context).
        let schema = json!({ "type": "object" });
        let mut content = json!({ "error": "connection refused" });
        enrich_failure(&mut content, "connection refused", Some(&schema));
        assert_eq!(content["error_kind"], "network");
        assert!(content.get("expected_schema").is_none());
    }

    #[test]
    fn oversized_schema_is_compacted() {
        // Build a schema whose serialized form exceeds the echo budget.
        let mut props = serde_json::Map::new();
        for index in 0..200 {
            props.insert(
                format!("field_{index}"),
                json!({ "type": "string", "description": "x".repeat(40) }),
            );
        }
        let schema = json!({ "type": "object", "required": ["field_0"], "properties": props });
        let mut content = json!({ "error": "invalid input: missing required field" });
        enrich_failure(
            &mut content,
            "invalid input: missing required field",
            Some(&schema),
        );

        let echoed = &content["expected_schema"];
        // Compacted: top-level type/required kept, properties summarized to types
        // (a string), and the whole thing is smaller than the raw schema.
        assert_eq!(echoed["type"], "object");
        assert_eq!(echoed["required"], json!(["field_0"]));
        assert_eq!(echoed["properties"]["field_0"], json!("string"));
        assert!(echoed.to_string().len() < schema.to_string().len());
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
