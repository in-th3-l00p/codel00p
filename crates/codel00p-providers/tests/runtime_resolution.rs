use codel00p_providers::{
    ApiMode, ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError,
    default_registry,
};

#[test]
fn client_resolve_returns_an_inspectable_route() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("or", Credential::api_key("openrouter-key"))
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openrouter");
    assert_eq!(route.requested_provider, "openrouter");
    assert_eq!(route.api_mode, ApiMode::ChatCompletions);
    assert_eq!(route.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(route.credential_source.as_deref(), Some("configured"));
}

#[test]
fn client_resolve_preserves_the_requested_alias() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("claude", Credential::api_key("anthropic-key"))
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("claude", "claude-sonnet-4.6")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "anthropic");
    assert_eq!(route.requested_provider, "claude");
    assert_eq!(route.api_mode, ApiMode::AnthropicMessages);
}

#[test]
fn client_resolve_reports_missing_base_url_before_credentials() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("key"))
        .build();

    let error = client
        .resolve(
            &InferenceRequest::builder("custom", "local-model")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(matches!(error, ProviderError::MissingBaseUrl { provider } if provider == "custom"));
}

