use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ToolDefinition, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn chat_completions_sends_tools_and_normalizes_tool_calls() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "user", "content": "What files changed?"}
                    ],
                    "tools": [{
                        "type": "function",
                        "function": {
                            "name": "git_status",
                            "description": "Read the repository status",
                            "parameters": {
                                "type": "object",
                                "properties": {},
                                "additionalProperties": false
                            }
                        }
                    }]
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "git_status",
                                "arguments": "{\"short\":true}"
                            }
                        }]
                    }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("What files changed?"))
                .tool(ToolDefinition::function(
                    "git_status",
                    "Read the repository status",
                    json!({
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }),
                ))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id.as_deref(), Some("call_123"));
    assert_eq!(response.tool_calls[0].name, "git_status");
    assert_eq!(response.tool_calls[0].arguments, json!({"short": true}));
}

