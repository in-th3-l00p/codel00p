//! In-turn error self-correction (perfect-coding-agent #12 T0.4).
//!
//! These exercise the harness behavior that turns repeated tool-call failures
//! into deliberate recovery: a classified `error_kind` + `hint` on failed
//! results, and a bounded "step back / replan" nudge once the same operation
//! fails `failure_budget` times in a row. A scripted model drives the
//! done-points; a controllable failing tool produces deterministic failures.

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, HarnessError, HarnessEvent, HarnessInferenceResponse, ModelToolCall,
    SelfCorrectConfig, Tool, ToolRegistry, ToolResult, UserMessage, Workspace,
};
use codel00p_protocol::SessionId;
use serde_json::{Value, json};
use support::ScriptedModelClient;
use tempfile::tempdir;

/// A tool that always fails with a fixed, classifiable message (a
/// missing-dependency signature), via a returned `Err`.
struct AlwaysFailTool;

#[async_trait]
impl Tool for AlwaysFailTool {
    fn name(&self) -> &str {
        "always_fail"
    }
    fn description(&self) -> &str {
        "A tool that always fails (test fixture)."
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _ws: &Workspace, _input: Value) -> Result<ToolResult, HarnessError> {
        Err(HarnessError::ToolFailed {
            name: "always_fail".to_string(),
            message: "bash: frobnicate: command not found".to_string(),
        })
    }
}

/// A tool that fails the first N calls (with a returned `Err`) then succeeds.
struct FailThenSucceedTool {
    fail_times: usize,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for FailThenSucceedTool {
    fn name(&self) -> &str {
        "flaky"
    }
    fn description(&self) -> &str {
        "Fails the first N times then succeeds (test fixture)."
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _ws: &Workspace, _input: Value) -> Result<ToolResult, HarnessError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n < self.fail_times {
            Err(HarnessError::ToolFailed {
                name: "flaky".to_string(),
                message: "permission denied".to_string(),
            })
        } else {
            Ok(ToolResult::json(json!({ "ok": true })))
        }
    }
}

/// A tool that always rejects its input as invalid, with a distinctive schema we
/// can assert is echoed back to the model.
struct InvalidInputTool;

#[async_trait]
impl Tool for InvalidInputTool {
    fn name(&self) -> &str {
        "needs_path"
    }
    fn description(&self) -> &str {
        "Always reports invalid input (test fixture)."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": { "path": { "type": "string" } }
        })
    }
    async fn execute(&self, _ws: &Workspace, _input: Value) -> Result<ToolResult, HarnessError> {
        Err(HarnessError::InvalidToolInput {
            name: "needs_path".to_string(),
            message: "missing string field `path`".to_string(),
        })
    }
}

/// A tool that records how many times it ran, used to prove a salvaged call
/// actually executes.
struct RecordingTool {
    runs: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for RecordingTool {
    fn name(&self) -> &str {
        "record"
    }
    fn description(&self) -> &str {
        "Records that it ran (test fixture)."
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, _ws: &Workspace, _input: Value) -> Result<ToolResult, HarnessError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        Ok(ToolResult::json(json!({ "ok": true })))
    }
}

fn call(id: &str, name: &str) -> ModelToolCall {
    ModelToolCall::new(id, name, json!({}))
}

/// All toggles on, budget 3.
fn enabled() -> SelfCorrectConfig {
    SelfCorrectConfig {
        error_hints: true,
        replan_on_failure: true,
        failure_budget: 3,
    }
}

fn nudges(events: &[HarnessEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, HarnessEvent::FailureBudgetExceeded { .. }))
        .count()
}

/// The same operation failing `failure_budget` times in a row trips exactly one
/// replan nudge (a `FailureBudgetExceeded` event + an injected user message).
#[tokio::test]
async fn repeated_same_failure_trips_replan_nudge() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    // Four identical failing calls; the model then finishes.
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a1", "always_fail")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a2", "always_fail")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a3", "always_fail")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a4", "always_fail")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "I give up gracefully."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(AlwaysFailTool))
        .self_correct_config(enabled())
        .max_iterations(20)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("nudge"), UserMessage::new("go"))
        .await
        .expect("run");

    // Budget is 3 → the nudge fires exactly on the 3rd consecutive failure.
    assert_eq!(nudges(&outcome.events), 1, "exactly one replan nudge");
    let event = outcome
        .events
        .iter()
        .find_map(|e| match e {
            HarnessEvent::FailureBudgetExceeded {
                operation,
                attempts,
                error_kind,
                ..
            } => Some((operation.clone(), *attempts, error_kind.clone())),
            _ => None,
        })
        .expect("FailureBudgetExceeded event");
    assert_eq!(event.0, "always_fail");
    assert_eq!(event.1, 3);
    assert_eq!(event.2, "missing_dependency");

    // The nudge was injected into the conversation as a user message.
    let injected =
        outcome.session_state.messages().iter().any(|m| {
            format!("{m:?}").contains("attempted") && format!("{m:?}").contains("3 times")
        });
    assert!(injected, "step-back nudge injected into the transcript");
}

/// Error hints enrich the failed tool result with a classified kind + hint.
#[tokio::test]
async fn error_hints_enrich_failed_results() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a1", "always_fail")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(AlwaysFailTool))
        .self_correct_config(enabled())
        .max_iterations(10)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("hints"), UserMessage::new("go"))
        .await
        .expect("run");

    let result = &outcome.tool_calls[0].result;
    assert_eq!(result.content()["error_kind"], "missing_dependency");
    assert!(
        result.content()["hint"]
            .as_str()
            .unwrap()
            .contains("dependency")
    );
}

/// An invalid-input failure echoes the failing tool's expected schema back to
/// the model, so it can match the shape and self-correct instead of looping —
/// the recovery lever for a mis-shaped tool call.
#[tokio::test]
async fn invalid_input_failure_echoes_tool_schema() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a1", "needs_path")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(InvalidInputTool))
        .self_correct_config(enabled())
        .max_iterations(10)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("schema"), UserMessage::new("go"))
        .await
        .expect("run");

    let result = &outcome.tool_calls[0].result;
    assert_eq!(result.content()["error_kind"], "invalid_input");
    // The exact schema the model must match is attached to the failed result.
    assert_eq!(
        result.content()["expected_schema"],
        InvalidInputTool.input_schema()
    );
    assert!(
        result.content()["hint"]
            .as_str()
            .unwrap()
            .contains("expected_schema")
    );
}

/// An invalid-input failure is graced once before it counts toward the replan
/// budget: with the schema echoed back, the model deserves a clean retry, and
/// "replan / try a different tool" is the wrong recovery for a fixable shape. So
/// `failure_budget` (3) invalid-input failures trip NO nudge — unlike a
/// substantive failure, which trips at exactly 3.
#[tokio::test]
async fn invalid_input_failure_is_graced_before_replan() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a1", "needs_path")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a2", "needs_path")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a3", "needs_path")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(InvalidInputTool))
        .self_correct_config(enabled())
        .max_iterations(20)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("grace"), UserMessage::new("go"))
        .await
        .expect("run");

    assert_eq!(
        nudges(&outcome.events),
        0,
        "the first invalid-input failure is graced, so 3 do not trip the budget"
    );
}

/// A tool call the model writes as message *text* (a fenced JSON object naming
/// an advertised tool) instead of a structured call is salvaged and executed,
/// so the turn acts on it rather than ending with the work silently dropped.
#[tokio::test]
async fn tool_call_emitted_as_text_is_salvaged_and_run() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let runs = Arc::new(AtomicUsize::new(0));

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::assistant(
            "github",
            "gpt-4o",
            "Running it now:\n```json\n{\"name\": \"record\", \"arguments\": {}}\n```",
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(RecordingTool { runs: runs.clone() }))
        .self_correct_config(enabled())
        .max_iterations(20)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("salvage"), UserMessage::new("go"))
        .await
        .expect("run");

    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "salvaged call executed once"
    );
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "record");
}

/// A genuine textual answer (no JSON tool-call object) ends the turn normally —
/// salvage must not fire on prose, even when it mentions a tool name.
#[tokio::test]
async fn prose_answer_is_not_salvaged() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let runs = Arc::new(AtomicUsize::new(0));

    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "You could use the record tool here, but I'll just explain it instead.",
    )]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(RecordingTool { runs: runs.clone() }))
        .self_correct_config(enabled())
        .max_iterations(20)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("prose"), UserMessage::new("go"))
        .await
        .expect("run");

    assert_eq!(
        runs.load(Ordering::SeqCst),
        0,
        "prose must not trigger a tool run"
    );
    assert!(outcome.tool_calls.is_empty());
    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("You could use the record tool here, but I'll just explain it instead.")
    );
}

/// `error_hints = false` ⇒ the failed result is bare (no error_kind/hint).
#[tokio::test]
async fn error_hints_off_leaves_bare_error() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a1", "always_fail")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(AlwaysFailTool))
        .self_correct_config(SelfCorrectConfig {
            error_hints: false,
            replan_on_failure: true,
            failure_budget: 3,
        })
        .max_iterations(10)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("bare"), UserMessage::new("go"))
        .await
        .expect("run");

    let result = &outcome.tool_calls[0].result;
    assert!(result.content().get("error").is_some());
    assert!(
        result.content().get("error_kind").is_none(),
        "no classification when off"
    );
    assert!(result.content().get("hint").is_none());
}

/// `replan_on_failure = false` ⇒ no nudge even past the budget.
#[tokio::test]
async fn replan_off_suppresses_nudge() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a1", "always_fail")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a2", "always_fail")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a3", "always_fail")],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![call("a4", "always_fail")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(AlwaysFailTool))
        .self_correct_config(SelfCorrectConfig {
            error_hints: true,
            replan_on_failure: false,
            failure_budget: 3,
        })
        .max_iterations(20)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("noreplan"), UserMessage::new("go"))
        .await
        .expect("run");

    assert_eq!(
        nudges(&outcome.events),
        0,
        "no nudge when replan_on_failure is off"
    );
}

/// A tool that fails once then succeeds does not trip the nudge, and the failure
/// streak resets on success.
#[tokio::test]
async fn fail_once_then_succeed_resets_and_no_nudge() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let calls = Arc::new(AtomicUsize::new(0));
    let tool = FailThenSucceedTool {
        fail_times: 1,
        calls: calls.clone(),
    };

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls("github", "gpt-4o", vec![call("f1", "flaky")]),
        HarnessInferenceResponse::with_tool_calls("github", "gpt-4o", vec![call("f2", "flaky")]),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "done"),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(tool))
        .self_correct_config(enabled())
        .max_iterations(10)
        .build()
        .expect("build")
        .run_turn(SessionId::from_static("flaky"), UserMessage::new("go"))
        .await
        .expect("run");

    assert_eq!(
        nudges(&outcome.events),
        0,
        "one failure then success → no nudge"
    );
    // First call failed (enriched), second succeeded.
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert_eq!(outcome.tool_calls[1].result.content()["ok"], true);
}
