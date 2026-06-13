use super::*;

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
    assert_eq!(route.base_url_source, RouteValueSource::ProviderDefault);
    assert!(route.capabilities.tools);
    assert!(route.capabilities.vision);
    assert_eq!(
        route.models_url.as_deref(),
        Some("https://api.anthropic.com/v1/models")
    );
}

#[test]
fn client_resolve_marks_request_base_url_overrides() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("key"))
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("custom", "local-model")
                .base_url("http://127.0.0.1:11434/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "custom");
    assert_eq!(route.base_url, "http://127.0.0.1:11434/v1");
    assert_eq!(route.base_url_source, RouteValueSource::RequestOverride);
    assert_eq!(route.policy_decision, ProviderPolicyDecision::Allowed);
    assert!(route.capabilities.tools);
    assert_eq!(route.models_url, None);
}

#[test]
fn client_resolve_uses_configured_provider_proxy() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "gpt",
            "https://proxy.codel00p.example/openai/v1",
            Credential::api_key("proxy-key"),
        )
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openai");
    assert_eq!(route.base_url, "https://proxy.codel00p.example/openai/v1");
    assert_eq!(route.base_url_source, RouteValueSource::CloudProxy);
    assert_eq!(route.credential_source.as_deref(), Some("cloud_proxy"));
    assert_eq!(route.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(route.policy_decision, ProviderPolicyDecision::Allowed);
}

#[test]
fn client_resolve_preserves_request_base_url_over_provider_proxy() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "openai",
            "https://proxy.codel00p.example/openai/v1",
            Credential::api_key("proxy-key"),
        )
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .base_url("https://one-off.example/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openai");
    assert_eq!(route.base_url, "https://one-off.example/v1");
    assert_eq!(route.base_url_source, RouteValueSource::RequestOverride);
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
