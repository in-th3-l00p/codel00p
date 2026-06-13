use super::support::*;

#[tokio::test]
async fn list_model_catalog_enforces_credential_source_kind_policy() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("configured-key"))
        .policy(
            ProviderPolicy::allow_all().with_allowed_credential_source_kinds(
                "custom",
                [CredentialSourceKind::ManagedIdentity],
            ),
        )
        .build();

    let error = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url("http://127.0.0.1:1/models")
                .build(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("credential source kind is not allowed"))
    );
}

#[tokio::test]
async fn list_model_catalog_reports_credential_source_kind_policy_metadata() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/managed-source-kind-models")
                .header("authorization", "Bearer managed-token");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "managed-source-kind-model", "name": "Managed Source Kind Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .managed_identity_credential(
            "custom",
            Credential::api_key("managed-token"),
            "azure/mi-prod",
        )
        .policy(
            ProviderPolicy::allow_all().with_allowed_credential_source_kinds(
                "custom",
                [CredentialSourceKind::ManagedIdentity],
            ),
        )
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/managed-source-kind-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(
        result.policy.allowed_credential_source_kinds,
        Some(vec![CredentialSourceKind::ManagedIdentity])
    );
    assert_eq!(result.models[0].id, "managed-source-kind-model");
}

#[tokio::test]
async fn list_model_catalog_enforces_credential_kind_policy() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::None)
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_credential_kinds("custom", [CredentialKind::ApiKey]),
        )
        .build();

    let error = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url("http://127.0.0.1:1/models")
                .build(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("credential kind is not allowed"))
    );
}

#[tokio::test]
async fn list_model_catalog_reports_credential_kind_policy_metadata() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/managed-models")
                .header("authorization", "Bearer managed-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "managed-model", "name": "Managed Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("managed-key"))
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_credential_kinds("custom", [CredentialKind::ApiKey]),
        )
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/managed-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(
        result.policy.allowed_credential_kinds,
        Some(vec![CredentialKind::ApiKey])
    );
    assert_eq!(result.models[0].id, "managed-model");
}
