use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, ResponseFormat,
    ToolCall, ToolDefinition, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn gemini_serializes_json_response_format_in_generation_config() {
    let server = MockServer::start_async().await;
    let generate = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1beta/models/gemini-2.5-flash:generateContent")
                .body_includes(r#""responseMimeType":"application/json""#)
                .body_includes(r#""responseSchema""#);
            then.status(200).json_body(json!({
                "candidates": [{
                    "content": { "role": "model", "parts": [{"text": "{}"}] },
                    "finishReason": "STOP"
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gemini", Credential::api_key("test-key"))
        .build();

    client
        .complete(
            InferenceRequest::builder("gemini", "gemini-2.5-flash")
                .base_url(server.base_url())
                .message(ChatMessage::user("give me json"))
                .response_format(ResponseFormat::JsonSchema {
                    name: "result".into(),
                    schema: json!({ "type": "object" }),
                })
                .build(),
        )
        .await
        .unwrap();

    generate.assert_async().await;
}

#[tokio::test]
async fn gemini_posts_payload_and_normalizes_text_response() {
    let server = MockServer::start_async().await;
    let generate = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1beta/models/gemini-2.5-flash:generateContent")
                .header("x-goog-api-key", "test-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "systemInstruction": {
                        "parts": [{"text": "You are precise."}]
                    },
                    "contents": [{
                        "role": "user",
                        "parts": [{"text": "Say hello."}]
                    }],
                    "generationConfig": {
                        "temperature": 0.2,
                        "maxOutputTokens": 64
                    }
                }));

            then.status(200).json_body(json!({
                "responseId": "resp_text",
                "modelVersion": "gemini-2.5-flash",
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "hello"}]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {
                    "promptTokenCount": 10,
                    "cachedContentTokenCount": 3,
                    "candidatesTokenCount": 4,
                    "thoughtsTokenCount": 2,
                    "totalTokenCount": 16
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gemini", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("gemini", "gemini-2.5-flash")
                .base_url(server.base_url())
                .message(ChatMessage::system("You are precise."))
                .message(ChatMessage::user("Say hello."))
                .temperature(0.2)
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    generate.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("STOP"));
    assert_eq!(
        response.provider_data.get("response_id"),
        Some(&json!("resp_text"))
    );
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 7);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cache_read_tokens, 3);
    assert_eq!(usage.reasoning_tokens, 2);
}

#[tokio::test]
async fn gemini_sends_tools_and_normalizes_function_calls() {
    let server = MockServer::start_async().await;
    let generate = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1beta/models/gemini-2.5-flash:generateContent")
                .json_body(json!({
                    "contents": [{
                        "role": "user",
                        "parts": [{"text": "What files changed?"}]
                    }],
                    "generationConfig": {
                        "maxOutputTokens": 64
                    },
                    "tools": [{
                        "functionDeclarations": [{
                            "name": "git_status",
                            "description": "Read the repository status",
                            "parameters": {
                                "type": "object",
                                "properties": {},
                                "additionalProperties": false
                            }
                        }]
                    }]
                }));

            then.status(200).json_body(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            {"text": "I will inspect git status."},
                            {
                                "functionCall": {
                                    "id": "call_123",
                                    "name": "git_status",
                                    "args": {"short": true}
                                }
                            }
                        ]
                    },
                    "finishReason": "STOP"
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gemini", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("gemini", "gemini-2.5-flash")
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

    generate.assert_async().await;
    assert_eq!(
        response.content.as_deref(),
        Some("I will inspect git status.")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("STOP"));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id.as_deref(), Some("call_123"));
    assert_eq!(response.tool_calls[0].name, "git_status");
    assert_eq!(response.tool_calls[0].arguments, json!({"short": true}));
}

#[tokio::test]
async fn gemini_sends_prior_function_calls_and_responses() {
    let server = MockServer::start_async().await;
    let generate = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1beta/models/gemini-2.5-flash:generateContent")
                .json_body(json!({
                    "contents": [
                        {
                            "role": "user",
                            "parts": [{"text": "Read README."}]
                        },
                        {
                            "role": "model",
                            "parts": [{
                                "functionCall": {
                                    "id": "call-readme",
                                    "name": "read_file",
                                    "args": {"path": "README.md"}
                                }
                            }]
                        },
                        {
                            "role": "user",
                            "parts": [{
                                "functionResponse": {
                                    "id": "call-readme",
                                    "name": "read_file",
                                    "response": {"content": "Agent Harness"}
                                }
                            }]
                        }
                    ],
                    "generationConfig": {
                        "maxOutputTokens": 64
                    }
                }));

            then.status(200).json_body(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "done"}]
                    },
                    "finishReason": "STOP"
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gemini", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("gemini", "gemini-2.5-flash")
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

    generate.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("done"));
}

#[tokio::test]
async fn gemini_requires_credentials_for_remote_requests() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("gemini", "gemini-2.5-flash")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::MissingCredential { provider } if provider == "gemini"));
}

#[tokio::test]
async fn gemini_streams_text_parts_and_usage() {
    use std::sync::{Arc, Mutex};

    let server = MockServer::start_async().await;
    let sse = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":2}}\n\n",
    );
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1beta/models/gemini-2.5-flash:streamGenerateContent")
                .query_param("alt", "sse")
                .header("x-goog-api-key", "test-key");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse);
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gemini", Credential::api_key("test-key"))
        .build();

    let tokens: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = tokens.clone();
    let sink = move |t: &str| collector.lock().unwrap().push(t.to_string());

    let response = client
        .complete_streaming(
            InferenceRequest::builder("gemini", "gemini-2.5-flash")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
            &sink,
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(*tokens.lock().unwrap(), vec!["Hel", "lo"]);
    assert_eq!(response.content.as_deref(), Some("Hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("STOP"));
    assert_eq!(response.usage.unwrap().output_tokens, 2);
}
