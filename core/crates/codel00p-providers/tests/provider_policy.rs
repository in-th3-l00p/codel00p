use codel00p_providers::{
    AuthType, ChatMessage, Credential, CredentialKind, CredentialSourceKind, InferenceClient,
    InferenceRequest, ProviderCapabilities, ProviderError, ProviderPolicy, RouteValueSource,
    default_registry,
};

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

#[test]
fn enterprise_direct_policy_template_allows_direct_corporate_providers() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .policy(ProviderPolicy::enterprise_direct())
        .build();

    let cases = [
        ("openai", "gpt-5", None),
        ("anthropic", "claude-sonnet-4.6", None),
        (
            "azure-foundry",
            "team-chat",
            Some("https://team.openai.azure.com"),
        ),
        ("bedrock", "anthropic.claude-3-5-haiku-20241022-v1:0", None),
        ("gemini", "gemini-2.5-flash", None),
        ("github", "gpt-4o-mini-2024-07-18", None),
        ("github-models", "openai/gpt-4o-mini", None),
    ];

    for (provider, model, base_url) in cases {
        let mut builder =
            InferenceRequest::builder(provider, model).message(ChatMessage::user("hello"));
        if let Some(base_url) = base_url {
            builder = builder.base_url(base_url);
        }

        let route = client.resolve(&builder.build()).unwrap();

        assert_eq!(route.provider, provider);
    }
}

#[test]
fn enterprise_direct_policy_template_blocks_brokers_and_custom_endpoints() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .policy(ProviderPolicy::enterprise_direct())
        .build();

    let openrouter = client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();
    assert!(
        matches!(openrouter, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );

    let custom = client
        .resolve(
            &InferenceRequest::builder("custom", "local-model")
                .base_url("http://127.0.0.1:11434/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();
    assert!(
        matches!(custom, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("not allowed"))
    );
}

#[test]
fn enterprise_direct_agentic_template_keeps_direct_provider_boundary() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .policy(ProviderPolicy::enterprise_direct_agentic())
        .build();

    let direct = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(direct.provider, "openai");

    let broker = client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(broker, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );
}

#[test]
fn enterprise_cloud_proxy_policy_template_requires_proxied_direct_routes() {
    let proxied_client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "gpt",
            "https://team-proxy.example/v1",
            Credential::api_key("proxy-key"),
        )
        .policy(ProviderPolicy::enterprise_cloud_proxy())
        .build();

    let route = proxied_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openai");
    assert_eq!(route.auth_type, AuthType::CloudProxy);
    assert_eq!(route.base_url_source, RouteValueSource::CloudProxy);
    assert_eq!(
        route.policy.allowed_auth_types,
        Some(vec![AuthType::CloudProxy])
    );

    let direct_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("configured-key"))
        .policy(ProviderPolicy::enterprise_cloud_proxy())
        .build();

    let direct = direct_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(direct, ProviderError::PolicyDenied { provider, reason } if provider == "openai" && reason.contains("auth type is not allowed"))
    );

    let broker_client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "openrouter",
            "https://team-proxy.example/openrouter",
            Credential::api_key("proxy-key"),
        )
        .policy(ProviderPolicy::enterprise_cloud_proxy())
        .build();

    let broker = broker_client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(broker, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );
}

#[test]
fn enterprise_managed_identity_policy_template_requires_managed_identity_credentials() {
    let managed_client = InferenceClient::builder()
        .registry(default_registry())
        .managed_identity_credential(
            "openai",
            Credential::api_key("managed-token"),
            "azure/workload-prod",
        )
        .policy(ProviderPolicy::enterprise_managed_identity())
        .build();

    let route = managed_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openai");
    assert_eq!(
        route.credential_source_kind,
        Some(CredentialSourceKind::ManagedIdentity)
    );
    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::ManagedIdentity])
    );

    let configured_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("configured-key"))
        .policy(ProviderPolicy::enterprise_managed_identity())
        .build();

    let configured = configured_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(configured, ProviderError::PolicyDenied { provider, reason } if provider == "openai" && reason.contains("credential source kind is not allowed"))
    );

    let broker_client = InferenceClient::builder()
        .registry(default_registry())
        .managed_identity_credential(
            "openrouter",
            Credential::api_key("managed-token"),
            "azure/openrouter-workload",
        )
        .policy(ProviderPolicy::enterprise_managed_identity())
        .build();

    let broker = broker_client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(broker, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );
}

#[test]
fn enterprise_organization_credentials_policy_template_requires_organization_credentials() {
    let organization_client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "openai",
            Credential::api_key("organization-token"),
            "team-ai/openai-prod",
        )
        .policy(ProviderPolicy::enterprise_organization_credentials())
        .build();

    let route = organization_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "openai");
    assert_eq!(
        route.credential_source_kind,
        Some(CredentialSourceKind::Organization)
    );
    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::Organization])
    );

    let configured_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("configured-key"))
        .policy(ProviderPolicy::enterprise_organization_credentials())
        .build();

    let configured = configured_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(configured, ProviderError::PolicyDenied { provider, reason } if provider == "openai" && reason.contains("credential source kind is not allowed"))
    );

    let broker_client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "openrouter",
            Credential::api_key("organization-token"),
            "team-ai/openrouter-prod",
        )
        .policy(ProviderPolicy::enterprise_organization_credentials())
        .build();

    let broker = broker_client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(broker, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );
}

#[test]
fn enterprise_custom_gateway_policy_template_requires_configured_gateway_provider() {
    let gateway_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("gateway-key"))
        .policy(ProviderPolicy::enterprise_custom_gateway())
        .build();

    let route = gateway_client
        .resolve(
            &InferenceRequest::builder("custom", "team/gpt-5")
                .base_url("https://ai-gateway.example/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(route.provider, "custom");
    assert_eq!(route.auth_type, AuthType::Custom);
    assert_eq!(route.base_url_source, RouteValueSource::RequestOverride);
    assert_eq!(
        route.policy.allowed_auth_types,
        Some(vec![AuthType::Custom])
    );

    let direct_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", Credential::api_key("configured-key"))
        .policy(ProviderPolicy::enterprise_custom_gateway())
        .build();

    let direct = direct_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(direct, ProviderError::PolicyDenied { provider, reason } if provider == "openai" && reason.contains("not allowed"))
    );

    let broker_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("broker-key"))
        .policy(ProviderPolicy::enterprise_custom_gateway())
        .build();

    let broker = broker_client
        .resolve(
            &InferenceRequest::builder("openrouter", "openai/gpt-test")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(broker, ProviderError::PolicyDenied { provider, reason } if provider == "openrouter" && reason.contains("not allowed"))
    );
}
