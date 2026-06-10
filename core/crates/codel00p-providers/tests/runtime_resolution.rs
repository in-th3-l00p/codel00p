use codel00p_providers::{
    ApiMode, ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError,
    ProviderPolicyDecision, RouteValueSource, default_registry,
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
    assert_eq!(route.policy_decision, ProviderPolicyDecision::Allowed);
    assert!(route.capabilities.tools);
    assert!(route.capabilities.streaming);
    assert_eq!(
        route.models_url.as_deref(),
        Some("https://openrouter.ai/api/v1/models")
    );
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
    });
}
