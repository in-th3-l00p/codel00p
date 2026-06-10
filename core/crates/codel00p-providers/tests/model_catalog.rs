use codel00p_providers::{
    Credential, InferenceClient, ModelCatalogRequest, ProviderError, ProviderPolicy,
    default_registry,
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
async fn list_models_normalizes_github_models_catalog() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/catalog/models")
                .header("authorization", "Bearer github-models-key");
            then.status(200).json_body(json!([
                {
                    "id": "openai/gpt-4.1-mini",
                    "name": "OpenAI GPT-4.1 Mini",
                    "publisher": "OpenAI",
                    "summary": "Fast model for everyday tasks",
                    "rate_limit_tier": "low",
                    "capabilities": ["chat", "tool-calling"]
                }
            ]));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github-models", Credential::api_key("github-models-key"))
        .build();

    let models = client
        .list_models(
            ModelCatalogRequest::builder("github-models")
                .models_url(format!("{}/catalog/models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "openai/gpt-4.1-mini");
    assert_eq!(
        models[0].display_name.as_deref(),
        Some("OpenAI GPT-4.1 Mini")
    );
    assert_eq!(models[0].owned_by.as_deref(), Some("OpenAI"));
    assert_eq!(
        models[0].provider_data.get("publisher"),
        Some(&json!("OpenAI"))
    );
    assert_eq!(
        models[0].provider_data.get("summary"),
        Some(&json!("Fast model for everyday tasks"))
    );
    assert_eq!(
        models[0].provider_data.get("rate_limit_tier"),
        Some(&json!("low"))
    );
    assert_eq!(
        models[0].provider_data.get("capabilities"),
        Some(&json!(["chat", "tool-calling"]))
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
