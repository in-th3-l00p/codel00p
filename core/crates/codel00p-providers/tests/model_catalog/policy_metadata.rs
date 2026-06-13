use super::support::*;

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
