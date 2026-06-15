//! Cooperative turn cancellation: the run loop stops at turn boundaries when the
//! signal is set, returning a `cancelled` outcome that still carries (and so can
//! persist) the work done so far.

mod support;

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, CancelSignal, HarnessError, HarnessInferenceRequest, HarnessInferenceResponse,
    ModelClient, ModelToolCall, SessionId, ToolRegistry, UserMessage, Workspace,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

#[tokio::test]
async fn a_pre_cancelled_turn_makes_no_inference_calls() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Should never be produced.",
    )]);

    let cancel = CancelSignal::new();
    cancel.cancel();

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .cancel_signal(cancel)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-cancel"),
            UserMessage::new("Go."),
        )
        .await
        .expect("run turn");

    assert!(outcome.cancelled, "outcome should be marked cancelled");
    assert_eq!(outcome.assistant_message, None);
    assert_eq!(model.requests().len(), 0, "no inference should run");
    // The turn still emits a completion event so listeners see it end cleanly.
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, codel00p_harness::HarnessEvent::TurnCompleted { .. })),
        "a cancelled turn still emits TurnCompleted"
    );
}

/// A model client that requests cancellation the moment it is asked to infer,
/// then returns a tool call — so the loop sees the cancel after inference but
/// before it executes any tool.
struct CancelOnInfer {
    signal: CancelSignal,
}

#[async_trait]
impl ModelClient for CancelOnInfer {
    async fn infer(
        &self,
        _request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        self.signal.cancel();
        Ok(HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ))
    }
}

#[tokio::test]
async fn cancelling_during_inference_stops_before_running_tools() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "hi\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let cancel = CancelSignal::new();

    let outcome = AgentHarness::builder()
        .model_client(CancelOnInfer {
            signal: cancel.clone(),
        })
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .cancel_signal(cancel)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-cancel-2"),
            UserMessage::new("Read it."),
        )
        .await
        .expect("run turn");

    assert!(outcome.cancelled);
    // The tool was never executed, so no tool call is recorded.
    assert!(outcome.tool_calls.is_empty(), "no tool should have run");
}
