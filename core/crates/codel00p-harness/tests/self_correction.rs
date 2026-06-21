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
