use super::*;

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
    assert_eq!(route.base_url_source, RouteValueSource::ProviderDefault);
    assert_eq!(route.credential_source.as_deref(), Some("configured"));
    assert_eq!(route.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(route.policy_decision, ProviderPolicyDecision::Allowed);
    assert!(route.capabilities.tools);
    assert!(route.capabilities.streaming);
    assert_eq!(
        route.models_url.as_deref(),
        Some("https://openrouter.ai/api/v1/models")
    );
}

#[test]
fn client_resolve_reports_credential_kind_metadata() {
    let api_key_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("openrouter-key"))
        .build();

    let api_key_route = api_key_client
        .resolve(
            &InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(api_key_route.credential_kind, Some(CredentialKind::ApiKey));

    let proxy_client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "openai",
            "https://proxy.codel00p.example/openai/v1",
            Credential::api_key("proxy-key"),
        )
        .build();

    let proxy_route = proxy_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(proxy_route.credential_kind, Some(CredentialKind::ApiKey));

    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "env-access-key");
            std::env::set_var("CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY", "env-secret-key");
            std::env::set_var("CODEL00P_PROVIDER_AWS_REGION", "eu-central-1");
        }

        let bedrock_client = InferenceClient::builder()
            .registry(default_registry())
            .credentials_from_env()
            .build();

        let bedrock_route = bedrock_client
            .resolve(
                &InferenceRequest::builder("bedrock", "anthropic.claude-3-5-sonnet")
                    .message(ChatMessage::user("hello"))
                    .build(),
            )
            .unwrap();

        assert_eq!(
            bedrock_route.credential_kind,
            Some(CredentialKind::AwsSigV4)
        );
    });
}

#[test]
fn client_resolve_reports_credential_source_kind_metadata() {
    let configured_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("openrouter-key"))
        .build();

    let configured_route = configured_client
        .resolve(
            &InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(
        configured_route.credential_source_kind,
        Some(CredentialSourceKind::Configured)
    );

    let proxy_client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "openai",
            "https://proxy.codel00p.example/openai/v1",
            Credential::api_key("proxy-key"),
        )
        .build();

    let proxy_route = proxy_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(
        proxy_route.credential_source_kind,
        Some(CredentialSourceKind::CloudProxy)
    );

    let managed_client = InferenceClient::builder()
        .registry(default_registry())
        .managed_identity_credential(
            "openai",
            Credential::api_key("managed-token"),
            "azure/workload-prod",
        )
        .build();

    let managed_route = managed_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(
        managed_route.credential_source_kind,
        Some(CredentialSourceKind::ManagedIdentity)
    );
    assert_eq!(
        managed_route.credential_source.as_deref(),
        Some("managed_identity:azure/workload-prod")
    );

    let organization_client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "openai",
            Credential::api_key("organization-token"),
            "team-ai/openai-prod",
        )
        .build();

    let organization_route = organization_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(
        organization_route.credential_source_kind,
        Some(CredentialSourceKind::Organization)
    );

    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_OPENAI_API_KEY", "env-openai-key");
        }

        let env_client = InferenceClient::builder()
            .registry(default_registry())
            .credentials_from_env()
            .build();

        let env_route = env_client
            .resolve(
                &InferenceRequest::builder("openai", "gpt-5-mini")
                    .message(ChatMessage::user("hello"))
                    .build(),
            )
            .unwrap();

        assert_eq!(
            env_route.credential_source_kind,
            Some(CredentialSourceKind::Environment)
        );
    });
}

#[test]
fn client_builder_can_resolve_managed_identity_credentials() {
    struct StaticManagedIdentityResolver;

    impl ManagedIdentityCredentialResolver for StaticManagedIdentityResolver {
        fn resolve(
            &self,
            request: ManagedIdentityCredentialRequest<'_>,
        ) -> Result<Credential, ProviderError> {
            assert_eq!(request.provider(), "openai");
            assert_eq!(request.identity_ref(), "azure/workload-prod");
            Ok(Credential::api_key("managed-token"))
        }
    }

    let client = InferenceClient::builder()
        .registry(default_registry())
        .managed_identity_credential_from_resolver(
            "openai",
            "azure/workload-prod",
            &StaticManagedIdentityResolver,
        )
        .expect("resolve managed identity credential")
        .policy(ProviderPolicy::enterprise_managed_identity())
        .build();

    let route = client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    assert_eq!(
        route.credential_source_kind,
        Some(CredentialSourceKind::ManagedIdentity)
    );
    assert_eq!(
        route.credential_source.as_deref(),
        Some("managed_identity:azure/workload-prod")
    );
    assert_eq!(route.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::ManagedIdentity])
    );
}

#[test]
fn client_resolve_reports_auth_type_metadata() {
    let direct_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github", Credential::api_key("github-token"))
        .build();

    let github_route = direct_client
        .resolve(
            &InferenceRequest::builder("github", "gpt-4o-mini-2024-07-18")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(github_route.auth_type, AuthType::GitHubCopilot);

    let proxy_client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "openai",
            "https://proxy.codel00p.example/openai/v1",
            Credential::api_key("proxy-key"),
        )
        .build();

    let proxy_route = proxy_client
        .resolve(
            &InferenceRequest::builder("openai", "gpt-5-mini")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();
    assert_eq!(proxy_route.auth_type, AuthType::CloudProxy);
}
