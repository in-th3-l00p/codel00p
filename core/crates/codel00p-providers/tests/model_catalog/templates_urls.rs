use super::support::*;

#[tokio::test]
async fn list_model_catalog_applies_enterprise_agentic_template() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/catalog/models")
                .header("authorization", "Bearer github-models-key");
            then.status(200).json_body(json!([
                {
                    "id": "openai/agentic-model",
                    "name": "Agentic Model",
                    "capabilities": ["tool-calling", "streaming", "reasoning"]
                },
                {
                    "id": "openai/tool-model",
                    "name": "Tool Model",
                    "capabilities": ["tool-calling"]
                },
                {
                    "id": "openai/stream-model",
                    "name": "Stream Model",
                    "capabilities": ["streaming"]
                }
            ]));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github-models", Credential::api_key("github-models-key"))
        .policy(ProviderPolicy::enterprise_direct_agentic())
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("github-models")
                .models_url(format!("{}/catalog/models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(result.provider, "github-models");
    assert!(result.policy.required_capabilities.tools);
    assert!(result.policy.required_capabilities.streaming);
    assert!(!result.policy.required_capabilities.vision);
    assert!(result.policy.required_capabilities.reasoning);
    assert_eq!(result.catalog_model_count, 3);
    assert_eq!(result.returned_model_count, 1);
    assert_eq!(result.models.len(), 1);
    assert_eq!(result.models[0].id, "openai/agentic-model");
}

#[tokio::test]
async fn list_model_catalog_reports_catalog_url_source() {
    let server = MockServer::start_async().await;
    let explicit_catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/explicit-models")
                .header("authorization", "Bearer test-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "explicit-model", "name": "Explicit Model"}
                ]
            }));
        })
        .await;
    let base_url_catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/models")
                .header("authorization", "Bearer test-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "base-model", "name": "Base Model"}
                ]
            }));
        })
        .await;
    let provider_default_catalog = server
        .mock_async(|when, then| {
            when.method(GET).path("/provider-default-models");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "default-model", "name": "Default Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let explicit = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/explicit-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();
    assert_eq!(
        explicit.models_url_source,
        ModelCatalogUrlSource::RequestModelsUrl
    );
    assert_eq!(explicit.models[0].id, "explicit-model");

    let base_url = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .base_url(server.base_url())
                .build(),
        )
        .await
        .unwrap();
    assert_eq!(
        base_url.models_url_source,
        ModelCatalogUrlSource::RequestBaseUrl
    );
    assert_eq!(base_url.models[0].id, "base-model");

    let default_models_url: &'static str =
        Box::leak(format!("{}/provider-default-models", server.base_url()).into_boxed_str());
    let provider_default_client = InferenceClient::builder()
        .registry(ProviderRegistry::new().register(ProviderProfile {
            id: "catalog-default",
            aliases: &[],
            display_name: "Catalog Default",
            description: "Test provider with default catalog URL",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::Custom,
            env_vars: &[],
            default_base_url: None,
            models_url: Some(default_models_url),
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        }))
        .credential("catalog-default", Credential::None)
        .build();

    let provider_default = provider_default_client
        .list_model_catalog(ModelCatalogRequest::builder("catalog-default").build())
        .await
        .unwrap();
    assert_eq!(
        provider_default.models_url_source,
        ModelCatalogUrlSource::ProviderDefault
    );
    assert_eq!(provider_default.credential_kind, Some(CredentialKind::None));
    assert_eq!(provider_default.models[0].id, "default-model");

    explicit_catalog.assert_async().await;
    base_url_catalog.assert_async().await;
    provider_default_catalog.assert_async().await;
}
