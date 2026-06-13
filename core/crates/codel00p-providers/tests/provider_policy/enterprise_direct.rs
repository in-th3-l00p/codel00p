use super::*;

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
