mod support;

use codel00p_harness::{
    HarnessInferenceRequest, HarnessInferenceResponse, ModelClient, ModelToolCall, SessionId,
    SessionState,
};
use serde_json::json;
use support::ScriptedModelClient;

#[tokio::test]
async fn scripted_model_client_records_requests_and_returns_responses() {
    let client = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "done",
    )]);
    let request =
        HarnessInferenceRequest::new(SessionState::new(SessionId::from_static("session-model")));

    let response = client.infer(request).await.expect("model response");

    assert_eq!(response.assistant_message(), Some("done"));
    assert_eq!(client.requests().len(), 1);
}

#[test]
fn inference_response_can_carry_tool_calls() {
    let response = HarnessInferenceResponse::with_tool_calls(
        "github",
        "gpt-4o",
        vec![ModelToolCall::new(
            "call-1",
            "read_file",
            json!({ "path": "README.md" }),
        )],
    );

    assert_eq!(response.provider(), "github");
    assert_eq!(response.model(), "gpt-4o");
    assert_eq!(response.tool_calls()[0].name(), "read_file");
}
