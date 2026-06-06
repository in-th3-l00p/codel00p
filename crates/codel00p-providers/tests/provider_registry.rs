use codel00p_providers::{
    ApiMode, AuthType, ChatMessage, Credential, InferenceClient, InferenceRequest,
    default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[test]
fn default_registry_contains_the_initial_corporate_provider_set() {
    let registry = default_registry();

    let expected = [
        ("openai", "OpenAI", ApiMode::Responses, AuthType::ApiKey),
        (
            "anthropic",
            "Anthropic",
            ApiMode::AnthropicMessages,
            AuthType::ApiKey,
        ),
        (
            "azure-foundry",
            "Azure AI Foundry",
            ApiMode::ChatCompletions,
            AuthType::ApiKey,
        ),
        (
            "bedrock",
            "AWS Bedrock",
            ApiMode::BedrockConverse,
            AuthType::AwsSdk,
        ),
        ("gemini", "Google Gemini", ApiMode::Gemini, AuthType::ApiKey),
        (
            "github",
            "GitHub Models",
            ApiMode::ChatCompletions,
            AuthType::GitHubCopilot,
        ),
        (
            "openrouter",
            "OpenRouter",
            ApiMode::ChatCompletions,
            AuthType::ApiKey,
        ),
        (
            "custom",
            "Custom OpenAI-compatible",
            ApiMode::ChatCompletions,
            AuthType::Custom,
        ),
    ];

    for (id, display_name, api_mode, auth_type) in expected {
        let profile = registry.resolve(id).unwrap();
        assert_eq!(profile.id, id);
        assert_eq!(profile.display_name, display_name);
        assert_eq!(profile.api_mode, api_mode);
        assert_eq!(profile.auth_type, auth_type);
    }
}

#[test]
fn default_registry_resolves_common_corporate_aliases() {
    let registry = default_registry();

    assert_eq!(registry.resolve("claude").unwrap().id, "anthropic");
    assert_eq!(registry.resolve("aws").unwrap().id, "bedrock");
    assert_eq!(registry.resolve("google").unwrap().id, "gemini");
    assert_eq!(registry.resolve("github-copilot").unwrap().id, "github");
    assert_eq!(registry.resolve("vllm").unwrap().id, "custom");
}

#[tokio::test]
async fn client_credentials_can_be_registered_by_alias() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer alias-key");
            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "ok"}
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("ollama", Credential::api_key("alias-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "local-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("ok"));
}

