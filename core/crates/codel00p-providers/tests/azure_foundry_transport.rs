use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn azure_foundry_posts_to_deployment_chat_completions() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/openai/deployments/team-chat/chat/completions")
                .query_param("api-version", "2024-10-21")
                .header("api-key", "azure-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "messages": [
                        {"role": "user", "content": "Say hello."}
                    ],
                    "temperature": 0.2,
                    "max_tokens": 128
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello from azure"
                    }
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 4,
                    "total_tokens": 14,
                    "prompt_tokens_details": {
                        "cached_tokens": 2
                    }
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("azure", Credential::api_key("azure-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("azure", "gpt-4.1")
                .base_url(server.base_url())
                .deployment("team-chat")
                .api_version("2024-10-21")
                .message(ChatMessage::user("Say hello."))
                .temperature(0.2)
                .max_output_tokens(128)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello from azure"));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 8);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cache_read_tokens, 2);
}

#[tokio::test]
async fn azure_foundry_defaults_deployment_and_api_version() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/openai/deployments/gpt-4.1-mini/chat/completions")
                .query_param("api-version", "2024-10-21")
                .header("api-key", "azure-key")
                .json_body(json!({
                    "messages": [
                        {"role": "user", "content": "Say hello."}
                    ]
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
        .credential("azure", Credential::api_key("azure-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("azure", "gpt-4.1-mini")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
}

#[tokio::test]
async fn azure_foundry_requires_credentials() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("azure", "gpt-4.1")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::MissingCredential { provider } if provider == "azure-foundry")
    );
}
