use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn chat_completions_posts_openai_compatible_payload_and_normalizes_response() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "system", "content": "You are precise."},
                        {"role": "user", "content": "Say hello."}
                    ],
                    "temperature": 0.2,
                    "max_tokens": 128
                }));

            then.status(200).json_body(json!({
                "id": "chatcmpl_test",
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello"
                    }
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 3,
                    "total_tokens": 13,
                    "prompt_tokens_details": {
                        "cached_tokens": 4
                    }
                }
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
                .message(ChatMessage::system("You are precise."))
                .message(ChatMessage::user("Say hello."))
                .temperature(0.2)
                .max_output_tokens(128)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 6);
    assert_eq!(usage.output_tokens, 3);
    assert_eq!(usage.cache_read_tokens, 4);
}

#[tokio::test]
async fn chat_completions_requires_credentials_for_remote_requests() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::MissingCredential { provider } if provider == "custom"));
}

#[tokio::test]
async fn github_copilot_uses_max_completion_tokens() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body(json!({
                    "model": "gpt-5.4-mini",
                    "messages": [
                        {"role": "user", "content": "Say hello."}
                    ],
                    "max_completion_tokens": 64
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello"
                    }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("github", "gpt-5.4-mini")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
}
