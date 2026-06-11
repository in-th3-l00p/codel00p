use codel00p_providers::{
    AzureManagedIdentityCredentialResolver, ChatMessage, CredentialKind, CredentialSourceKind,
    InferenceClient, InferenceRequest, ProviderError, ProviderPolicy, default_registry,
};
use httpmock::Method::GET;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn azure_managed_identity_resolver_fetches_token_and_preserves_route_metadata() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/metadata/identity/oauth2/token")
                .header("metadata", "true")
                .query_param("api-version", "2018-02-01")
                .query_param("resource", "https://cognitiveservices.azure.com/")
                .query_param("client_id", "client-123");

            then.status(200).json_body(json!({
                "access_token": "managed-token",
                "expires_in": "3599",
                "expires_on": "1893456000",
                "resource": "https://cognitiveservices.azure.com/",
                "token_type": "Bearer"
            }));
        })
        .await;

    let resolver = AzureManagedIdentityCredentialResolver::user_assigned_client_id(
        "https://cognitiveservices.azure.com/",
        "client-123",
    )
    .with_endpoint(format!(
        "{}/metadata/identity/oauth2/token",
        server.base_url()
    ));

    let client = InferenceClient::builder()
        .registry(default_registry())
        .azure_managed_identity_credential_from_resolver(
            "azure-foundry",
            "azure/workload-prod",
            &resolver,
        )
        .await
        .expect("resolve Azure managed identity credential")
        .policy(ProviderPolicy::enterprise_managed_identity())
        .build();

    token.assert_async().await;

    let route = client
        .resolve(
            &InferenceRequest::builder("azure-foundry", "gpt-4o-prod")
                .base_url("https://example.services.ai.azure.com/models/chat/completions")
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

#[tokio::test]
async fn azure_managed_identity_resolver_rejects_missing_access_token() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/metadata/identity/oauth2/token")
                .header("metadata", "true")
                .query_param("api-version", "2018-02-01")
                .query_param("resource", "https://cognitiveservices.azure.com/");

            then.status(200).json_body(json!({
                "expires_in": "3599",
                "token_type": "Bearer"
            }));
        })
        .await;

    let resolver = AzureManagedIdentityCredentialResolver::system_assigned(
        "https://cognitiveservices.azure.com/",
    )
    .with_endpoint(format!(
        "{}/metadata/identity/oauth2/token",
        server.base_url()
    ));

    let error = resolver
        .resolve("azure-foundry", "azure/system-prod")
        .await
        .unwrap_err();

    token.assert_async().await;
    assert!(
        matches!(error, ProviderError::InvalidResponse { provider, message }
            if provider == "azure-foundry" && message.contains("access_token"))
    );
}
