use super::*;

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
