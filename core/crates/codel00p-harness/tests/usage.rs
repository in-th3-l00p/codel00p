mod support;

use codel00p_harness::{
    AgentHarness, HarnessEvent, HarnessInferenceResponse, SessionId, ToolRegistry, UserMessage,
    Workspace,
};
use codel00p_protocol::TokenUsage;
use support::ScriptedModelClient;
use tempfile::tempdir;

/// When the model response carries token usage, the harness threads it onto the
/// per-inference `InferenceCompleted` event and aggregates a turn total on
/// `TurnCompleted`.
#[tokio::test]
async fn usage_is_surfaced_on_inference_and_turn_events() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let usage = TokenUsage {
        input_tokens: 1234,
        output_tokens: 567,
        cache_read_tokens: 10,
        cache_write_tokens: 0,
        reasoning_tokens: 5,
    };
    let response = HarnessInferenceResponse::assistant("github", "gpt-4o", "Done.")
        .with_usage(Some(usage.clone()));
    let model = ScriptedModelClient::new(vec![response]);
    let session_id = SessionId::from_static("session-usage");

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(session_id, UserMessage::new("Do a thing."))
        .await
        .expect("run turn");

    let inference_usage = outcome.events.iter().find_map(|event| match event {
        HarnessEvent::InferenceCompleted { usage, .. } => usage.clone(),
        _ => None,
    });
    assert_eq!(
        inference_usage.as_ref(),
        Some(&usage),
        "InferenceCompleted should carry the response's usage"
    );

    let turn_usage = outcome.events.iter().find_map(|event| match event {
        HarnessEvent::TurnCompleted { usage, .. } => usage.clone(),
        _ => None,
    });
    assert_eq!(
        turn_usage.as_ref(),
        Some(&usage),
        "TurnCompleted should aggregate the single inference's usage"
    );
    let turn_usage = turn_usage.expect("turn usage present");
    assert_eq!(turn_usage.total_tokens(), 1816);
}

/// A turn whose inferences report no usage keeps the legacy (no-usage) shape so
/// providers that don't surface usage are unaffected.
#[tokio::test]
async fn usage_absent_when_response_omits_it() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);
    let session_id = SessionId::from_static("session-no-usage");

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(session_id, UserMessage::new("Do a thing."))
        .await
        .expect("run turn");

    assert!(outcome.events.iter().all(|event| matches!(
        event,
        HarnessEvent::InferenceCompleted { usage: None, .. }
    ) || !matches!(
        event,
        HarnessEvent::InferenceCompleted { .. }
    )));
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted {
            usage: None,
            cost: None,
            ..
        })
    ));
}
