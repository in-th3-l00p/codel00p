use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, ToolCall,
    ToolDefinition, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn bedrock_converse_posts_signed_payload_and_normalizes_text_response() {
    let server = MockServer::start_async().await;
    let converse = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/model/test-model/converse")
                .header("content-type", "application/json")
                .header("x-amz-security-token", "session-token")
                .header_matches(
                    "authorization",
                    "AWS4-HMAC-SHA256 Credential=test-access/[0-9]{8}/us-east-1/bedrock/aws4_request, SignedHeaders=.*",
                )
                .header_matches("x-amz-date", "[0-9]{8}T[0-9]{6}Z")
                .json_body(json!({
                    "system": [{"text": "You are precise."}],
                    "messages": [{
                        "role": "user",
                        "content": [{"text": "Say hello."}]
                    }],
                    "inferenceConfig": {
                        "temperature": 0.2,
                        "maxTokens": 64
                    }
                }));

            then.status(200).json_body(json!({
                "output": {
                    "message": {
                        "role": "assistant",
                        "content": [{"text": "hello"}]
                    }
                },
                "stopReason": "end_turn",
                "usage": {
                    "inputTokens": 10,
                    "outputTokens": 4,
                    "totalTokens": 14,
                    "cacheReadInputTokens": 3,
                    "cacheWriteInputTokens": 2
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential(
            "bedrock",
            Credential::aws_sigv4(
                "test-access",
                "test-secret",
                Some("session-token"),
                "us-east-1",
            ),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("bedrock", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::system("You are precise."))
                .message(ChatMessage::user("Say hello."))
                .temperature(0.2)
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    converse.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("end_turn"));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 7);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cache_read_tokens, 3);
    assert_eq!(usage.cache_write_tokens, 2);
}

#[tokio::test]
async fn bedrock_converse_sends_tools_and_normalizes_tool_use_response() {
    let server = MockServer::start_async().await;
    let converse = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/model/test-model/converse")
                .json_body(json!({
                    "messages": [{
                        "role": "user",
                        "content": [{"text": "What files changed?"}]
                    }],
                    "inferenceConfig": {
                        "maxTokens": 64
                    },
                    "toolConfig": {
                        "tools": [{
                            "toolSpec": {
                                "name": "git_status",
                                "description": "Read the repository status",
                                "inputSchema": {
                                    "json": {
                                        "type": "object",
                                        "properties": {},
                                        "additionalProperties": false
                                    }
                                }
                            }
                        }]
                    }
                }));

            then.status(200).json_body(json!({
                "output": {
                    "message": {
                        "role": "assistant",
                        "content": [
                            {"text": "I will inspect git status."},
                            {
                                "toolUse": {
                                    "toolUseId": "tooluse_123",
                                    "name": "git_status",
                                    "input": {"short": true}
                                }
                            }
                        ]
                    }
                },
                "stopReason": "tool_use"
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential(
            "bedrock",
            Credential::aws_sigv4("test-access", "test-secret", None, "us-east-1"),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("bedrock", "test-model")
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

    converse.assert_async().await;
    assert_eq!(
        response.content.as_deref(),
        Some("I will inspect git status.")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id.as_deref(), Some("tooluse_123"));
    assert_eq!(response.tool_calls[0].name, "git_status");
    assert_eq!(response.tool_calls[0].arguments, json!({"short": true}));
}

#[tokio::test]
async fn bedrock_converse_sends_prior_tool_use_and_tool_results() {
    let server = MockServer::start_async().await;
    let converse = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/model/test-model/converse")
                .json_body(json!({
                    "messages": [
                        {
                            "role": "user",
                            "content": [{"text": "Read README."}]
                        },
                        {
                            "role": "assistant",
                            "content": [{
                                "toolUse": {
                                    "toolUseId": "call-readme",
                                    "name": "read_file",
                                    "input": {"path": "README.md"}
                                }
                            }]
                        },
                        {
                            "role": "user",
                            "content": [{
                                "toolResult": {
                                    "toolUseId": "call-readme",
                                    "content": [{"text": "{\"content\":\"Agent Harness\"}"}]
                                }
                            }]
                        }
                    ],
                    "inferenceConfig": {
                        "maxTokens": 64
                    }
                }));

            then.status(200).json_body(json!({
                "output": {
                    "message": {
                        "role": "assistant",
                        "content": [{"text": "done"}]
                    }
                },
                "stopReason": "end_turn"
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential(
            "bedrock",
            Credential::aws_sigv4("test-access", "test-secret", None, "us-east-1"),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("bedrock", "test-model")
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

    converse.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("done"));
}

#[tokio::test]
async fn bedrock_converse_requires_credentials_for_remote_requests() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("bedrock", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::MissingCredential { provider } if provider == "bedrock")
    );
}

fn eventstream_frame(event_type: &str, payload: &serde_json::Value) -> Vec<u8> {
    let name = b":event-type";
    let value = event_type.as_bytes();
    let mut headers = Vec::new();
    headers.push(name.len() as u8);
    headers.extend_from_slice(name);
    headers.push(7u8); // string header value type
    headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
    headers.extend_from_slice(value);

    let payload_bytes = serde_json::to_vec(payload).unwrap();
    let total_len = 16 + headers.len() + payload_bytes.len();

    let mut frame = Vec::new();
    frame.extend_from_slice(&(total_len as u32).to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0, 0, 0]); // prelude CRC (ignored by decoder)
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(&payload_bytes);
    frame.extend_from_slice(&[0, 0, 0, 0]); // message CRC (ignored by decoder)
    frame
}

#[tokio::test]
async fn bedrock_streams_eventstream_text_tool_and_usage() {
    use std::sync::{Arc, Mutex};

    let server = MockServer::start_async().await;
    let mut body = Vec::new();
    body.extend(eventstream_frame(
        "contentBlockDelta",
        &json!({"contentBlockIndex": 0, "delta": {"text": "Hel"}}),
    ));
    body.extend(eventstream_frame(
        "contentBlockDelta",
        &json!({"contentBlockIndex": 0, "delta": {"text": "lo"}}),
    ));
    body.extend(eventstream_frame(
        "contentBlockStart",
        &json!({"contentBlockIndex": 1, "start": {"toolUse": {"toolUseId": "tu_1", "name": "read_file"}}}),
    ));
    body.extend(eventstream_frame(
        "contentBlockDelta",
        &json!({"contentBlockIndex": 1, "delta": {"toolUse": {"input": "{\"path\""}}}),
    ));
    body.extend(eventstream_frame(
        "contentBlockDelta",
        &json!({"contentBlockIndex": 1, "delta": {"toolUse": {"input": ":\"a.txt\"}"}}}),
    ));
    body.extend(eventstream_frame(
        "messageStop",
        &json!({"stopReason": "tool_use"}),
    ));
    body.extend(eventstream_frame(
        "metadata",
        &json!({"usage": {"inputTokens": 5, "outputTokens": 7}}),
    ));
    let chat = server
        .mock_async(|when, then| {
            when.method(POST).path("/model/test-model/converse-stream");
            then.status(200)
                .header("content-type", "application/vnd.amazon.eventstream")
                .body(body);
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential(
            "bedrock",
            Credential::aws_sigv4("test-access", "test-secret", None, "us-east-1"),
        )
        .build();

    let tokens: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = tokens.clone();
    let sink = move |t: &str| collector.lock().unwrap().push(t.to_string());

    let response = client
        .complete_streaming(
            InferenceRequest::builder("bedrock", "test-model")
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
