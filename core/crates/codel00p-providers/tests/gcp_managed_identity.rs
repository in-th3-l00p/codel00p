use codel00p_providers::{
    ChatMessage, CredentialKind, CredentialSourceKind, GcpManagedIdentityCredentialResolver,
    InferenceClient, InferenceRequest, ProviderError, ProviderPolicy, default_registry,
};
use httpmock::Method::GET;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn gcp_managed_identity_resolver_fetches_service_account_token() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/computeMetadata/v1/instance/service-accounts/default/token")
                .header("metadata-flavor", "Google");

            then.status(200).json_body(json!({
                "access_token": "gcp-access-token",
                "expires_in": 3599,
                "token_type": "Bearer"
            }));
        })
        .await;

    let resolver = GcpManagedIdentityCredentialResolver::default_service_account()
        .with_endpoint(server.base_url());

    let client = InferenceClient::builder()
        .registry(default_registry())
        .gcp_managed_identity_credential_from_resolver(
            "custom",
            "gcp/default-service-account",
            &resolver,
        )
        .await
        .expect("resolve GCP managed identity credential")
        .policy(
            ProviderPolicy::allow_all().with_allowed_credential_source_kinds(
                "custom",
                [CredentialSourceKind::ManagedIdentity],
            ),
        )
        .build();

    token.assert_async().await;

    let route = client
        .resolve(
            &InferenceRequest::builder("custom", "google/gemini-2.5-pro")
                .base_url("https://example.googleapis.com/v1")
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
        Some("managed_identity:gcp/default-service-account")
    );
    assert_eq!(route.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::ManagedIdentity])
    );
}

#[tokio::test]
async fn gcp_managed_identity_resolver_rejects_missing_access_token() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/computeMetadata/v1/instance/service-accounts/default/token")
                .header("metadata-flavor", "Google");

            then.status(200).json_body(json!({
                "expires_in": 3599,
                "token_type": "Bearer"
            }));
        })
        .await;

    let resolver = GcpManagedIdentityCredentialResolver::default_service_account()
        .with_endpoint(server.base_url());

    let error = resolver
        .resolve("custom", "gcp/default-service-account")
        .await
        .unwrap_err();

    token.assert_async().await;
    assert!(
        matches!(error, ProviderError::InvalidResponse { provider, message }
            if provider == "custom" && message.contains("access_token"))
    );
}
