use codel00p_providers::{
    ApiMode, AuthType, ChatMessage, Credential, CredentialKind, InferenceClient, InferenceRequest,
    ProviderCapabilities, ProviderError, ProviderPolicy, ProviderPolicyDecision, RouteValueSource,
    default_registry,
};

fn with_env_lock(test: impl FnOnce()) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let keys = [
        "CODEL00P_PROVIDER_OPENAI_API_KEY",
        "OPENAI_API_KEY",
        "CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID",
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "CODEL00P_PROVIDER_AWS_SESSION_TOKEN",
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ];
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
    test();
    for key in keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

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

#[test]
fn client_builder_loads_api_key_credentials_from_env() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_OPENAI_API_KEY", "env-openai-key");
        }

        let client = InferenceClient::builder()
            .registry(default_registry())
            .credentials_from_env()
            .build();

        let route = client
            .resolve(
                &InferenceRequest::builder("openai", "gpt-5-mini")
                    .message(ChatMessage::user("hello"))
                    .build(),
            )
            .unwrap();

        assert_eq!(
            route.credential_source.as_deref(),
            Some("environment:CODEL00P_PROVIDER_OPENAI_API_KEY")
        );
    });
}

#[test]
fn client_builder_preserves_organization_credential_source() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "gpt",
            Credential::api_key("managed-openai-key"),
            "team-ai/openai-prod",
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
        route.credential_source.as_deref(),
        Some("organization:team-ai/openai-prod")
    );
}

#[test]
fn client_builder_preserves_explicit_credentials_over_env() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_OPENAI_API_KEY", "env-openai-key");
        }

        let client = InferenceClient::builder()
            .registry(default_registry())
            .credential("openai", Credential::api_key("manual-openai-key"))
            .credentials_from_env()
            .build();

        let route = client
            .resolve(
                &InferenceRequest::builder("openai", "gpt-5-mini")
                    .message(ChatMessage::user("hello"))
                    .build(),
            )
            .unwrap();

        assert_eq!(route.credential_source.as_deref(), Some("configured"));
    });
}

#[test]
fn client_builder_loads_aws_sigv4_credentials_from_env() {
    with_env_lock(|| {
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "env-access-key");
            std::env::set_var("CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY", "env-secret-key");
            std::env::set_var("CODEL00P_PROVIDER_AWS_REGION", "eu-central-1");
        }

        let client = InferenceClient::builder()
            .registry(default_registry())
            .credentials_from_env()
            .build();

        let route = client
            .resolve(
                &InferenceRequest::builder("bedrock", "anthropic.claude-3-5-sonnet")
                    .message(ChatMessage::user("hello"))
                    .build(),
            )
            .unwrap();

        assert_eq!(
            route.credential_source.as_deref(),
            Some("environment:CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID")
        );
        assert_eq!(route.credential_kind, Some(CredentialKind::AwsSigV4));
    });
}
