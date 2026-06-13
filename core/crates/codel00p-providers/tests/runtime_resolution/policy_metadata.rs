use super::*;

#[test]
fn client_resolve_reports_route_policy_metadata() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gpt", Credential::api_key("openai-key"))
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_models("gpt", ["gpt-5-mini"])
                .with_allowed_credential_kinds("gpt", [CredentialKind::ApiKey])
                .with_required_provider_capabilities(
                    "gpt",
                    ProviderCapabilities {
                        tools: true,
                        reasoning: true,
                        ..ProviderCapabilities::default()
                    },
                )
                .with_required_model_capabilities(
                    "gpt",
                    ProviderCapabilities {
                        tools: true,
                        streaming: true,
                        ..ProviderCapabilities::default()
                    },
                ),
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
    assert_eq!(
        route.policy.allowed_models,
        Some(vec!["gpt-5-mini".to_string()])
    );
    assert_eq!(
        route.policy.allowed_credential_kinds,
        Some(vec![CredentialKind::ApiKey])
    );
    assert!(route.policy.required_provider_capabilities.tools);
    assert!(route.policy.required_provider_capabilities.reasoning);
    assert!(route.policy.required_model_capabilities.tools);
    assert!(route.policy.required_model_capabilities.streaming);
}

#[test]
fn client_resolve_reports_credential_source_kind_policy_metadata() {
    let client = InferenceClient::builder()
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

    let route = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::ManagedIdentity])
    );
}

#[test]
fn client_resolve_reports_auth_type_policy_metadata() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "gpt",
            "https://team-proxy.example/v1",
            Credential::api_key("proxy-key"),
        )
        .policy(ProviderPolicy::allow_all().with_allowed_auth_types("gpt", [AuthType::CloudProxy]))
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(
        route.policy.allowed_auth_types,
        Some(vec![AuthType::CloudProxy])
    );
}
