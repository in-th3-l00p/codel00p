use super::*;

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

#[test]
fn provider_policy_serializes_control_plane_defaults() {
    let policy = ProviderPolicy::enterprise_custom_gateway()
        .with_allowed_credential_source_kinds("custom", [CredentialSourceKind::Organization]);

    let encoded = serde_json::to_value(&policy).unwrap();
    assert_eq!(
        encoded,
        json!({
            "allowed_providers": ["custom"],
            "allowed_auth_types": {
                "custom": ["Custom"]
            },
            "allowed_credential_source_kinds": {
                "custom": ["Organization"]
            }
        })
    );

    let decoded: ProviderPolicy = serde_json::from_value(encoded).unwrap();
    let gateway_client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "custom",
            Credential::api_key("gateway-key"),
            "team-ai/custom-gateway",
        )
        .policy(decoded.clone())
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
    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::Organization])
    );

    let configured_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("configured-key"))
        .policy(decoded)
        .build();

    let configured = configured_client
        .resolve(
            &InferenceRequest::builder("custom", "team/gpt-5")
                .base_url("https://ai-gateway.example/v1")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap_err();

    assert!(
        matches!(configured, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("credential source kind is not allowed"))
    );
}

#[test]
fn provider_policy_presets_list_metadata_and_resolve_policies() {
    let preset_ids: Vec<_> = ProviderPolicy::presets()
        .iter()
        .map(|preset| preset.id)
        .collect();
    assert_eq!(
        preset_ids,
        vec![
            "allow_all",
            "enterprise_direct",
            "enterprise_cloud_proxy",
            "enterprise_custom_gateway",
            "enterprise_managed_identity",
            "enterprise_organization_credentials",
            "enterprise_direct_agentic"
        ]
    );

    let encoded = serde_json::to_value(ProviderPolicy::presets()).unwrap();
    assert_eq!(
        encoded[3],
        json!({
            "id": "enterprise_custom_gateway",
            "display_name": "Enterprise Custom Gateway",
            "description": "Allow only the configured OpenAI-compatible gateway profile."
        })
    );

    let gateway_policy = ProviderPolicy::from_preset("enterprise_custom_gateway").unwrap();
    let gateway_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("gateway-key"))
        .policy(gateway_policy)
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
    assert_eq!(
        route.policy.allowed_auth_types,
        Some(vec![AuthType::Custom])
    );
    assert!(ProviderPolicy::from_preset("unknown").is_none());
}
