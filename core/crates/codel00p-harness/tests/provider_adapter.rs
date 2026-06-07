use codel00p_harness::{
    HarnessInferenceRequest, ModelToolCall, ProviderModelClient, SessionId, SessionState,
    UserMessage,
};
use codel00p_providers::{InferenceResponse, MessageRole, ToolCall};
use serde_json::json;

#[test]
fn maps_harness_session_messages_to_provider_request() {
    let mut state = SessionState::new(SessionId::from_static("session-provider"));
    state.push_user(UserMessage::new("Read the README."));
    state.push_assistant("I need to inspect it.");
    state.push_tool_result("call-1", "read_file", r#"{"content":"Agent Harness"}"#);
    let request = HarnessInferenceRequest::new(state).with_runtime_context(
        "/workspace",
        vec!["read_file".to_string(), "search_text".to_string()],
    );

    let provider_request =
        ProviderModelClient::build_provider_request("github", "gpt-4o", &request);

    assert_eq!(provider_request.provider, "github");
    assert_eq!(provider_request.model, "gpt-4o");
    assert_eq!(provider_request.messages.len(), 3);
    assert_eq!(provider_request.messages[0].role, MessageRole::User);
    assert_eq!(provider_request.messages[1].role, MessageRole::Assistant);
    assert_eq!(provider_request.messages[2].role, MessageRole::Tool);
    assert_eq!(
        provider_request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["read_file", "search_text"]
    );
}

#[test]
fn maps_provider_response_to_harness_response() {
    let provider_response = InferenceResponse {
        content: Some("Need the README.".to_string()),
        tool_calls: vec![ToolCall {
            id: Some("call-provider".to_string()),
            name: "read_file".to_string(),
            arguments: json!({ "path": "README.md" }),
            provider_data: Default::default(),
        }],
        finish_reason: Some("tool_calls".to_string()),
        reasoning: None,
        usage: None,
        provider_data: Default::default(),
    };

    let response =
        ProviderModelClient::map_provider_response("github", "gpt-4o", provider_response);

    assert_eq!(response.provider(), "github");
    assert_eq!(response.model(), "gpt-4o");
    assert_eq!(response.assistant_message(), Some("Need the README."));
    assert_eq!(
        response.tool_calls(),
        &[ModelToolCall::new(
            "call-provider",
            "read_file",
            json!({ "path": "README.md" }),
        )]
    );
    assert_eq!(response.finish_reason(), Some("tool_calls"));
}
