use super::*;

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
