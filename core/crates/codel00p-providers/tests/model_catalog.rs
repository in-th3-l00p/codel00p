use codel00p_providers::{
    Credential, InferenceClient, ModelCatalogRequest, ProviderCapabilities, ProviderError,
    ProviderPolicy, ProviderPolicyDecision, default_registry,
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
                    {
                        "id": "gpt-test",
                        "object": "model",
                        "owned_by": "openai",
                        "description": "OpenAI-compatible test model"
                    },
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
    assert_eq!(
        models[0].description.as_deref(),
        Some("OpenAI-compatible test model")
    );
    assert_eq!(
        models[0].provider_data.get("description"),
        Some(&json!("OpenAI-compatible test model"))
    );
    assert_eq!(models[1].id, "claude-via-gateway");
    assert_eq!(
        models[1].display_name.as_deref(),
        Some("Claude via Gateway")
    );
    assert_eq!(
        models[1].provider_data.get("context_length"),
        Some(&json!(200000))
    );
    assert_eq!(models[1].limits.max_input_tokens, Some(200000));
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
                    "registry": "github",
                    "html_url": "https://github.com/marketplace/models/openai/gpt-4.1-mini",
                    "version": "2025-04-14",
                    "summary": "Fast model for everyday tasks",
                    "rate_limit_tier": "low",
                    "tags": ["reasoning", "multimodal"],
                    "capabilities": ["chat", "tool-calling"],
                    "limits": {
                        "max_input_tokens": 128000,
                        "max_output_tokens": 16384
                    },
                    "supported_input_modalities": ["text", "image"],
                    "supported_output_modalities": ["text"]
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
    assert_eq!(models[0].capabilities, vec!["chat", "tool-calling"]);
    assert!(models[0].capability_flags.tools);
    assert!(!models[0].capability_flags.streaming);
    assert!(models[0].capability_flags.vision);
    assert!(models[0].capability_flags.reasoning);
    assert_eq!(
        models[0].description.as_deref(),
        Some("Fast model for everyday tasks")
    );
    assert_eq!(models[0].annotations.registry.as_deref(), Some("github"));
    assert_eq!(
        models[0].annotations.html_url.as_deref(),
        Some("https://github.com/marketplace/models/openai/gpt-4.1-mini")
    );
    assert_eq!(models[0].annotations.version.as_deref(), Some("2025-04-14"));
    assert_eq!(
        models[0].annotations.rate_limit_tier.as_deref(),
        Some("low")
    );
    assert_eq!(models[0].annotations.tags, vec!["reasoning", "multimodal"]);
    assert_eq!(models[0].limits.max_input_tokens, Some(128000));
    assert_eq!(models[0].limits.max_output_tokens, Some(16384));
    assert_eq!(models[0].input_modalities, vec!["text", "image"]);
    assert_eq!(models[0].output_modalities, vec!["text"]);
    assert_eq!(
        models[0].provider_data.get("publisher"),
        Some(&json!("OpenAI"))
    );
    assert_eq!(
        models[0].provider_data.get("summary"),
        Some(&json!("Fast model for everyday tasks"))
    );
    assert_eq!(
        models[0].provider_data.get("registry"),
        Some(&json!("github"))
    );
    assert_eq!(
        models[0].provider_data.get("html_url"),
        Some(&json!(
            "https://github.com/marketplace/models/openai/gpt-4.1-mini"
        ))
    );
    assert_eq!(
        models[0].provider_data.get("version"),
        Some(&json!("2025-04-14"))
    );
    assert_eq!(
        models[0].provider_data.get("rate_limit_tier"),
        Some(&json!("low"))
    );
    assert_eq!(
        models[0].provider_data.get("tags"),
        Some(&json!(["reasoning", "multimodal"]))
    );
    assert_eq!(
        models[0].provider_data.get("capabilities"),
        Some(&json!(["chat", "tool-calling"]))
    );
    assert_eq!(
        models[0].provider_data.get("limits"),
        Some(&json!({
            "max_input_tokens": 128000,
            "max_output_tokens": 16384
        }))
    );
    assert_eq!(
        models[0].provider_data.get("supported_input_modalities"),
        Some(&json!(["text", "image"]))
    );
    assert_eq!(
        models[0].provider_data.get("supported_output_modalities"),
        Some(&json!(["text"]))
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

#[tokio::test]
async fn list_model_catalog_reports_policy_metadata() {
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
        .credential("local", Credential::api_key("test-key"))
        .policy(
            ProviderPolicy::allow_all()
                .with_allowed_models("local", ["tool-vision-model", "tool-text-model"])
                .with_required_model_capabilities(
                    "local",
                    ProviderCapabilities {
                        tools: true,
                        vision: true,
                        ..ProviderCapabilities::default()
                    },
                ),
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
    assert_eq!(result.requested_provider, "local");
    assert_eq!(result.provider, "custom");
    assert_eq!(result.models_url, format!("{}/models", server.base_url()));
    assert_eq!(result.policy_decision, ProviderPolicyDecision::Allowed);
    assert_eq!(
        result.policy.allowed_models.as_deref(),
        Some(
            [
                "tool-text-model".to_string(),
                "tool-vision-model".to_string()
            ]
            .as_slice()
        )
    );
    assert!(result.policy.required_capabilities.tools);
    assert!(!result.policy.required_capabilities.streaming);
    assert!(result.policy.required_capabilities.vision);
    assert!(!result.policy.required_capabilities.reasoning);
    assert_eq!(result.catalog_model_count, 3);
    assert_eq!(result.returned_model_count, 1);
    assert_eq!(result.models.len(), 1);
    assert_eq!(result.models[0].id, "tool-vision-model");
}

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
    assert_eq!(result.models.len(), 1);
    assert_eq!(result.models[0].id, "local-model");
}

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
