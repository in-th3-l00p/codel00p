use codel00p_providers::{
    Credential, InferenceClient, ModelCatalogRequest, ProviderError, default_registry,
};
use httpmock::Method::GET;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn list_models_fetches_and_normalizes_openai_compatible_catalog() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/models")
                .header("authorization", "Bearer test-key");
            then.status(200).json_body(json!({
                "object": "list",
                "data": [
                    {"id": "gpt-test", "object": "model", "owned_by": "openai"},
                    {"id": "claude-via-gateway", "name": "Claude via Gateway", "context_length": 200000}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let models = client
        .list_models(
            ModelCatalogRequest::builder("custom")
                .base_url(server.base_url())
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gpt-test");
    assert_eq!(models[0].owned_by.as_deref(), Some("openai"));
    assert_eq!(models[1].id, "claude-via-gateway");
    assert_eq!(
        models[1].display_name.as_deref(),
        Some("Claude via Gateway")
    );
    assert_eq!(
        models[1].provider_data.get("context_length"),
        Some(&json!(200000))
    );
}

#[tokio::test]
async fn list_models_reports_missing_catalog_configuration() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .list_models(ModelCatalogRequest::builder("custom").build())
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::MissingBaseUrl { provider } if provider == "custom"));
}

#[tokio::test]
async fn list_models_rejects_invalid_catalog_payload() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({ "models": [] }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let error = client
        .list_models(
            ModelCatalogRequest::builder("custom")
                .base_url(server.base_url())
                .build(),
        )
        .await
        .unwrap_err();

    catalog.assert_async().await;
    assert!(
        matches!(error, ProviderError::InvalidResponse { provider, .. } if provider == "custom")
    );
}
