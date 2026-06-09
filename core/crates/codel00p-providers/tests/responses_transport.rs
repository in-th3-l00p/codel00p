use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, ToolCall,
    ToolDefinition, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn responses_posts_payload_and_normalizes_text_response() {
    let server = MockServer::start_async().await;
    let responses = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .header("authorization", "Bearer test-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "model": "gpt-5-mini",
                    "input": [
                        {
                            "role": "system",
                            "content": [{"type": "input_text", "text": "You are precise."}]
                        },
                        {
                            "role": "developer",
                            "content": [{"type": "input_text", "text": "Use project memory first."}]
                        },
                        {
                            "role": "user",
                            "content": [{"type": "input_text", "text": "Say hello."}]
                        }
                    ],
                    "store": false,
                    "temperature": 0.2,
                    "max_output_tokens": 64
                }));

            then.status(200).json_body(json!({
                "id": "resp_text",
                "object": "response",
                "status": "completed",
                "model": "gpt-5-mini",
                "output": [{
                    "id": "msg_text",
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "hello", "annotations": []}
                    ]
                }],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 4,
                    "total_tokens": 14,
                    "input_tokens_details": {"cached_tokens": 3},
                    "output_tokens_details": {"reasoning_tokens": 2}
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("openai", "gpt-5-mini")
                .base_url(server.base_url())
                .message(ChatMessage::system("You are precise."))
                .message(ChatMessage {
                    role: codel00p_providers::MessageRole::Developer,
                    content: Some("Use project memory first.".to_string()),
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                })
                .message(ChatMessage::user("Say hello."))
                .temperature(0.2)
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    responses.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("completed"));
    assert_eq!(response.provider_data.get("id"), Some(&json!("resp_text")));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 7);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cache_read_tokens, 3);
    assert_eq!(usage.reasoning_tokens, 2);
}

#[tokio::test]
async fn responses_sends_tools_and_normalizes_function_calls() {
    let server = MockServer::start_async().await;
    let responses = server
        .mock_async(|when, then| {
            when.method(POST).path("/v1/responses").json_body(json!({
                "model": "gpt-5-mini",
                "input": [{
                    "role": "user",
                    "content": [{"type": "input_text", "text": "What files changed?"}]
                }],
                "store": false,
                "tools": [{
                    "type": "function",
                    "name": "git_status",
                    "description": "Read the repository status",
                    "parameters": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                }],
                "max_output_tokens": 64
            }));

            then.status(200).json_body(json!({
                "id": "resp_tool",
                "object": "response",
                "status": "completed",
                "model": "gpt-5-mini",
                "output": [
                    {
                        "id": "msg_tool",
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [
                            {"type": "output_text", "text": "I will inspect git status.", "annotations": []}
                        ]
                    },
                    {
                        "id": "fc_123",
                        "type": "function_call",
                        "status": "completed",
                        "call_id": "call_123",
                        "name": "git_status",
                        "arguments": "{\"short\":true}"
                    }
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("openai", "gpt-5-mini")
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

    responses.assert_async().await;
    assert_eq!(
        response.content.as_deref(),
        Some("I will inspect git status.")
    );
    assert_eq!(response.finish_reason.as_deref(), Some("completed"));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id.as_deref(), Some("call_123"));
    assert_eq!(response.tool_calls[0].name, "git_status");
    assert_eq!(response.tool_calls[0].arguments, json!({"short": true}));
}

#[tokio::test]
async fn responses_sends_prior_function_calls_and_outputs() {
    let server = MockServer::start_async().await;
    let responses = server
        .mock_async(|when, then| {
            when.method(POST).path("/v1/responses").json_body(json!({
                "model": "gpt-5-mini",
                "input": [
                    {
                        "role": "user",
                        "content": [{"type": "input_text", "text": "Read README."}]
                    },
                    {
                        "type": "function_call",
                        "call_id": "call-readme",
                        "name": "read_file",
                        "arguments": "{\"path\":\"README.md\"}"
                    },
                    {
                        "type": "function_call_output",
                        "call_id": "call-readme",
                        "output": "{\"content\":\"Agent Harness\"}"
                    }
                ],
                "store": false,
                "max_output_tokens": 64
            }));

            then.status(200).json_body(json!({
                "id": "resp_done",
                "object": "response",
                "status": "completed",
                "model": "gpt-5-mini",
                "output": [{
                    "id": "msg_done",
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "done", "annotations": []}
                    ]
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("openai", "gpt-5-mini")
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

    responses.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("done"));
}

#[tokio::test]
async fn responses_requires_credentials_for_remote_requests() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("openai", "gpt-5-mini")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::MissingCredential { provider } if provider == "openai"));
}
