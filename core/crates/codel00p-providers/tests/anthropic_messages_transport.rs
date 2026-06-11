use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, ToolCall,
    ToolDefinition, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn anthropic_messages_posts_payload_and_normalizes_text_response() {
    let server = MockServer::start_async().await;
    let messages = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/messages")
                .header("x-api-key", "test-key")
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json_body(json!({
                    "model": "claude-3-5-sonnet-latest",
                    "max_tokens": 128,
                    "system": "Project memory:\n- Run pnpm verify before pushing.",
                    "messages": [
                        {"role": "user", "content": "Inspect this project."}
                    ],
                    "temperature": 0.2
                }));

            then.status(200).json_body(json!({
                "id": "msg_test",
                "type": "message",
                "role": "assistant",
                "model": "claude-3-5-sonnet-latest",
                "content": [
                    {"type": "text", "text": "Use codel00p memory."}
                ],
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 7,
                    "cache_read_input_tokens": 3,
                    "cache_creation_input_tokens": 2
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("anthropic", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("anthropic", "claude-3-5-sonnet-latest")
                .base_url(server.base_url())
                .message(ChatMessage::system(
                    "Project memory:\n- Run pnpm verify before pushing.",
                ))
                .message(ChatMessage::user("Inspect this project."))
                .temperature(0.2)
                .max_output_tokens(128)
                .build(),
        )
        .await
        .unwrap();

    messages.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("Use codel00p memory."));
    assert_eq!(response.finish_reason.as_deref(), Some("end_turn"));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 12);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.cache_read_tokens, 3);
    assert_eq!(usage.cache_write_tokens, 2);
}

#[tokio::test]
async fn anthropic_messages_sends_tools_and_normalizes_tool_use_response() {
    let server = MockServer::start_async().await;
    let messages = server
        .mock_async(|when, then| {
            when.method(POST).path("/v1/messages").json_body(json!({
                "model": "claude-3-5-sonnet-latest",
                "max_tokens": 64,
                "messages": [
                    {"role": "user", "content": "What files changed?"}
                ],
                "tools": [{
                    "name": "git_status",
                    "description": "Read the repository status",
                    "input_schema": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                }]
            }));

            then.status(200).json_body(json!({
                "id": "msg_tool",
                "type": "message",
                "role": "assistant",
                "model": "claude-3-5-sonnet-latest",
                "content": [
                    {"type": "text", "text": "I will inspect git status."},
                    {
                        "type": "tool_use",
                        "id": "toolu_123",
                        "name": "git_status",
                        "input": {"short": true}
                    }
                ],
                "stop_reason": "tool_use",
                "usage": {
                    "input_tokens": 18,
                    "output_tokens": 11
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("anthropic", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("anthropic", "claude-3-5-sonnet-latest")
                .base_url(server.base_url())
                .message(ChatMessage::user("What files changed?"))
                .max_output_tokens(64)
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

    messages.assert_async().await;
    assert_eq!(
        response.content.as_deref(),
        Some("I will inspect git status.")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id.as_deref(), Some("toolu_123"));
    assert_eq!(response.tool_calls[0].name, "git_status");
    assert_eq!(response.tool_calls[0].arguments, json!({"short": true}));
}

#[tokio::test]
async fn anthropic_messages_sends_assistant_tool_calls_and_tool_results() {
    let server = MockServer::start_async().await;
    let messages = server
        .mock_async(|when, then| {
            when.method(POST).path("/v1/messages").json_body(json!({
                "model": "claude-3-5-sonnet-latest",
                "max_tokens": 64,
                "messages": [
                    {"role": "user", "content": "Read README."},
                    {
                        "role": "assistant",
                        "content": [{
                            "type": "tool_use",
                            "id": "call-readme",
                            "name": "read_file",
                            "input": {"path": "README.md"}
                        }]
                    },
                    {
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": "call-readme",
                            "content": "{\"content\":\"Agent Harness\"}"
                        }]
                    }
                ]
            }));

            then.status(200).json_body(json!({
                "id": "msg_done",
                "type": "message",
                "role": "assistant",
                "model": "claude-3-5-sonnet-latest",
                "content": [
                    {"type": "text", "text": "done"}
                ],
                "stop_reason": "end_turn"
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("anthropic", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("anthropic", "claude-3-5-sonnet-latest")
                .base_url(server.base_url())
                .message(ChatMessage::user("Read README."))
                .message(ChatMessage::assistant_tool_calls(vec![ToolCall {
                    id: Some("call-readme".to_string()),
                    name: "read_file".to_string(),
                    arguments: json!({ "path": "README.md" }),
                    provider_data: Default::default(),
                }]))
                .message(ChatMessage::tool_result(
                    "call-readme",
                    r#"{"content":"Agent Harness"}"#,
                ))
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    messages.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("done"));
}

#[tokio::test]
async fn anthropic_messages_requires_credentials_for_remote_requests() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("anthropic", "claude-3-5-sonnet-latest")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::MissingCredential { provider } if provider == "anthropic")
    );
}

#[tokio::test]
async fn anthropic_streams_text_and_tool_use_deltas() {
    use std::sync::{Arc, Mutex};

    let server = MockServer::start_async().await;
    let sse = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"read_file\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"a.txt\\\"}\"}}\n\n",
        "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":7}}\n\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/messages")
                .header("x-api-key", "test-key")
                .body_includes(r#""stream":true"#);
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse);
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("anthropic", Credential::api_key("test-key"))
        .build();

    let tokens: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = tokens.clone();
    let sink = move |t: &str| collector.lock().unwrap().push(t.to_string());

    let response = client
        .complete_streaming(
            InferenceRequest::builder("anthropic", "claude-3-5-sonnet-latest")
                .base_url(server.base_url())
                .message(ChatMessage::user("Read a.txt."))
                .build(),
            &sink,
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(*tokens.lock().unwrap(), vec!["Hel", "lo"]);
    assert_eq!(response.content.as_deref(), Some("Hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].name, "read_file");
    assert_eq!(response.tool_calls[0].arguments, json!({ "path": "a.txt" }));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 5);
    assert_eq!(usage.output_tokens, 7);
}
