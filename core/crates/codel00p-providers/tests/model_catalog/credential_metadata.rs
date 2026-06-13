use super::support::*;

#[tokio::test]
async fn list_model_catalog_reports_credential_source() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/models")
                .header("authorization", "Bearer managed-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "local-model", "name": "Local Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "local",
            Credential::api_key("managed-key"),
            "team-ai/local-catalog",
        )
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("local")
                .base_url(server.base_url())
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(
        result.credential_source.as_deref(),
        Some("organization:team-ai/local-catalog")
    );
    assert_eq!(result.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(result.models.len(), 1);
    assert_eq!(result.models[0].id, "local-model");
}

#[tokio::test]
async fn list_model_catalog_reports_credential_kind_metadata() {
    let server = MockServer::start_async().await;
    let managed_catalog = server
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
    let public_catalog = server
        .mock_async(|when, then| {
            when.method(GET).path("/public-models");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "public-model", "name": "Public Model"}
                ]
            }));
        })
        .await;

    let managed_client = InferenceClient::builder()
        .registry(default_registry())
        .organization_credential(
            "local",
            Credential::api_key("managed-key"),
            "team-ai/local-catalog",
        )
        .build();
    let managed = managed_client
        .list_model_catalog(
            ModelCatalogRequest::builder("local")
                .models_url(format!("{}/managed-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();
    assert_eq!(managed.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(managed.models[0].id, "managed-model");

    let public_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::None)
        .build();
    let public = public_client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/public-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();
    assert_eq!(public.credential_kind, Some(CredentialKind::None));
    assert_eq!(public.models[0].id, "public-model");

    managed_catalog.assert_async().await;
    public_catalog.assert_async().await;
}

#[tokio::test]
async fn list_model_catalog_reports_auth_type_metadata() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/auth-type-models")
                .header("authorization", "Bearer custom-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "custom-model", "name": "Custom Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("custom-key"))
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/auth-type-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(result.auth_type, AuthType::Custom);
    assert_eq!(result.credential_kind, Some(CredentialKind::ApiKey));
    assert_eq!(result.models[0].id, "custom-model");
}

#[tokio::test]
async fn list_model_catalog_reports_credential_source_kind_metadata() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/managed-identity-models")
                .header("authorization", "Bearer managed-token");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "managed-identity-model", "name": "Managed Identity Model"}
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
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/managed-identity-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(
        result.credential_source_kind,
        Some(CredentialSourceKind::ManagedIdentity)
    );
    assert_eq!(
        result.credential_source.as_deref(),
        Some("managed_identity:azure/mi-prod")
    );
    assert_eq!(result.models[0].id, "managed-identity-model");
}
