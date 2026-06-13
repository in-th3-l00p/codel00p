use super::*;

#[test]
fn provider_policy_blocks_disallowed_provider_before_inference() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("key"))
        .policy(ProviderPolicy::allow_only(["anthropic"]))
        .build();

    let error = client
        .resolve(
            &InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );
}

#[test]
fn provider_policy_accepts_aliases_but_enforces_canonical_provider_ids() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("claude", Credential::api_key("key"))
        .policy(ProviderPolicy::allow_only(["claude"]))
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("anthropic", "claude-sonnet-4.6")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "anthropic");
}

#[test]
fn provider_policy_blocks_disallowed_models_before_inference() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("key"))
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_models("openrouter", ["anthropic/claude-sonnet"]),
        )
        .build();

    let error = client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("model is not allowed"))
    );
}

#[test]
fn provider_policy_accepts_model_policy_provider_aliases() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("key"))
        .policy(ProviderPolicy::allow_all().with_allowed_models("or", ["anthropic/claude-sonnet"]))
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openrouter");
}

#[test]
fn provider_policy_enforces_credential_kind_policy() {
    let api_key_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gpt", Credential::api_key("openai-key"))
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_credential_kinds("gpt", [CredentialKind::ApiKey]),
        )
        .build();

    let route = api_key_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(route.provider, "openai");

    let unauthenticated_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::None)
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_credential_kinds("custom", [CredentialKind::ApiKey]),
        )
        .build();

    let error = unauthenticated_client
        .resolve(
            &InferenceRequest::builder("custom", "local-model")
                .base_url("http://127.0.0.1:11434/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("credential kind is not allowed"))
    );
}

#[test]
fn provider_policy_enforces_credential_source_kind_policy() {
    let managed_client = InferenceClient::builder()
        .registry(default_registry())
        .managed_identity_credential(
            "openai",
            Credential::api_key("managed-token"),
            "azure/workload-prod",
        )
        .policy(
            ProviderPolicy::allow_all().with_allowed_credential_source_kinds(
                "gpt",
                [CredentialSourceKind::ManagedIdentity],
            ),
        )
        .build();

    let route = managed_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(route.provider, "openai");

    let configured_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("configured-key"))
        .policy(
            ProviderPolicy::allow_all().with_allowed_credential_source_kinds(
                "gpt",
                [CredentialSourceKind::ManagedIdentity],
            ),
        )
        .build();

    let error = configured_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "openai" && reason.contains("credential source kind is not allowed"))
    );
}

#[test]
fn provider_policy_enforces_auth_type_policy() {
    let proxy_client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "gpt",
            "https://team-proxy.example/v1",
            Credential::api_key("proxy-key"),
        )
        .policy(ProviderPolicy::allow_all().with_allowed_auth_types("gpt", [AuthType::CloudProxy]))
        .build();

    let route = proxy_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(route.provider, "openai");
    assert_eq!(route.auth_type, AuthType::CloudProxy);

    let direct_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("configured-key"))
        .policy(ProviderPolicy::allow_all().with_allowed_auth_types("gpt", [AuthType::CloudProxy]))
        .build();

    let error = direct_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "openai" && reason.contains("auth type is not allowed"))
    );
}

#[test]
fn provider_policy_enforces_provider_capability_policy() {
    let agentic_client = InferenceClient::builder()
        .registry(default_registry())
        .policy(
            ProviderPolicy::allow_all().with_required_provider_capabilities(
                "gpt",
                ProviderCapabilities {
                    tools: true,
                    reasoning: true,
                    ..ProviderCapabilities::default()
                },
            ),
        )
        .build();

    let route = agentic_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(route.provider, "openai");

    let vision_client = InferenceClient::builder()
        .registry(default_registry())
        .policy(
            ProviderPolicy::allow_all().with_required_provider_capabilities(
                "custom",
                ProviderCapabilities {
                    vision: true,
                    ..ProviderCapabilities::default()
                },
            ),
        )
        .build();

    let error = vision_client
        .resolve(
            &InferenceRequest::builder("custom", "local-model")
                .base_url("http://127.0.0.1:11434/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("provider capabilities do not satisfy"))
    );
}
