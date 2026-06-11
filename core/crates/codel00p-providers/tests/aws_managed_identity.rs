use codel00p_providers::{
    AwsManagedIdentityCredentialResolver, ChatMessage, CredentialKind, CredentialSourceKind,
    InferenceClient, InferenceRequest, ProviderError, ProviderPolicy, default_registry,
};
use httpmock::Method::{GET, PUT};
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn aws_managed_identity_resolver_fetches_instance_profile_credentials() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/latest/api/token")
                .header("x-aws-ec2-metadata-token-ttl-seconds", "21600");

            then.status(200).body("imds-token");
        })
        .await;
    let role_list = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/latest/meta-data/iam/security-credentials/")
                .header("x-aws-ec2-metadata-token", "imds-token");

            then.status(200).body("bedrock-prod-role\n");
        })
        .await;
    let credentials = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/latest/meta-data/iam/security-credentials/bedrock-prod-role")
                .header("x-aws-ec2-metadata-token", "imds-token");

            then.status(200).json_body(json!({
                "Code": "Success",
                "AccessKeyId": "ASIAEXAMPLE",
                "SecretAccessKey": "secret",
                "Token": "session-token",
                "Expiration": "2026-06-11T16:00:00Z"
            }));
        })
        .await;

    let resolver = AwsManagedIdentityCredentialResolver::instance_profile("us-east-1")
        .with_endpoint(server.base_url());

    let client = InferenceClient::builder()
        .registry(default_registry())
        .aws_managed_identity_credential_from_resolver(
            "bedrock",
            "aws/instance-profile-prod",
            &resolver,
        )
        .await
        .expect("resolve AWS managed identity credential")
        .policy(ProviderPolicy::enterprise_managed_identity())
        .build();

    token.assert_async().await;
    role_list.assert_async().await;
    credentials.assert_async().await;

    let route = client
        .resolve(
            &InferenceRequest::builder("bedrock", "anthropic.claude-3-5-sonnet")
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
        Some("managed_identity:aws/instance-profile-prod")
    );
    assert_eq!(route.credential_kind, Some(CredentialKind::AwsSigV4));
    assert_eq!(
        route.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::ManagedIdentity])
    );
}

#[tokio::test]
async fn aws_managed_identity_resolver_rejects_incomplete_role_credentials() {
    let server = MockServer::start_async().await;
    let token = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/latest/api/token")
                .header("x-aws-ec2-metadata-token-ttl-seconds", "21600");

            then.status(200).body("imds-token");
        })
        .await;
    let credentials = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/latest/meta-data/iam/security-credentials/bedrock-prod-role")
                .header("x-aws-ec2-metadata-token", "imds-token");

            then.status(200).json_body(json!({
                "Code": "Success",
                "AccessKeyId": "ASIAEXAMPLE",
                "Token": "session-token",
                "Expiration": "2026-06-11T16:00:00Z"
            }));
        })
        .await;

    let resolver = AwsManagedIdentityCredentialResolver::instance_profile("us-east-1")
        .with_endpoint(server.base_url())
        .with_role_name("bedrock-prod-role");

    let error = resolver
        .resolve("bedrock", "aws/instance-profile-prod")
        .await
        .unwrap_err();

    token.assert_async().await;
    credentials.assert_async().await;
    assert!(
        matches!(error, ProviderError::InvalidResponse { provider, message }
            if provider == "bedrock" && message.contains("SecretAccessKey"))
    );
}
