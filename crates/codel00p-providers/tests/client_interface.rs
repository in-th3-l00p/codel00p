use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, InferenceResponse,
    MessageRole, ProviderError, default_registry,
};

#[tokio::test]
async fn client_builder_exposes_a_small_complete_api() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let request = InferenceRequest::builder("custom", "test-model")
        .message(ChatMessage::user("explain this repository"))
        .build();

    let error = client.complete(request).await.unwrap_err();

    assert!(matches!(error, ProviderError::MissingBaseUrl { provider } if provider == "custom"));
}

#[test]
fn default_registry_resolves_initial_provider_aliases() {
    let registry = default_registry();

    assert_eq!(registry.resolve("openai").unwrap().id, "openai");
    assert_eq!(registry.resolve("azure").unwrap().id, "azure-foundry");
    assert_eq!(registry.resolve("or").unwrap().id, "openrouter");
    assert_eq!(registry.resolve("ollama").unwrap().id, "custom");
}

#[test]
fn request_builder_preserves_a_clean_message_shape() {
    let request = InferenceRequest::builder("openai", "gpt-5")
        .message(ChatMessage::system("You are precise."))
        .message(ChatMessage::user("Summarize the memory plan."))
        .temperature(0.2)
        .max_output_tokens(2048)
        .build();

    assert_eq!(request.provider, "openai");
    assert_eq!(request.model, "gpt-5");
    assert_eq!(request.messages[0].role, MessageRole::System);
    assert_eq!(request.messages[1].content, "Summarize the memory plan.");
    assert_eq!(request.temperature, Some(0.2));
    assert_eq!(request.max_output_tokens, Some(2048));
}

#[test]
fn normalized_response_is_provider_neutral_but_keeps_provider_data() {
    let response = InferenceResponse::assistant("done")
        .with_finish_reason("stop")
        .with_provider_data("response_id", "resp_123");

    assert_eq!(response.content.as_deref(), Some("done"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response.provider_data.get("response_id").and_then(|value| value.as_str()),
        Some("resp_123")
    );
}
