use super::support::*;

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

#[tokio::test]
async fn list_models_filters_disallowed_models() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "allowed-model", "name": "Allowed"},
                    {"id": "blocked-model", "name": "Blocked"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .policy(ProviderPolicy::allow_all().with_allowed_models("custom", ["allowed-model"]))
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
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "allowed-model");
}

#[tokio::test]
async fn list_models_filters_by_required_capabilities() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET).path("/models");
            then.status(200).json_body(json!({
                "data": [
                    {
                        "id": "tool-vision-model",
                        "capabilities": ["tool-calling"],
                        "supported_input_modalities": ["text", "image"]
                    },
                    {
                        "id": "tool-text-model",
                        "capabilities": ["tool-calling"],
                        "supported_input_modalities": ["text"]
                    },
                    {
                        "id": "vision-only-model",
                        "supported_input_modalities": ["text", "image"]
                    }
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .policy(
            ProviderPolicy::allow_all().with_required_model_capabilities(
                "custom",
                ProviderCapabilities {
                    tools: true,
                    vision: true,
                    ..ProviderCapabilities::default()
                },
            ),
        )
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
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "tool-vision-model");
}
