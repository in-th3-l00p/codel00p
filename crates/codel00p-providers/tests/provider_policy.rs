use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, ProviderPolicy,
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
