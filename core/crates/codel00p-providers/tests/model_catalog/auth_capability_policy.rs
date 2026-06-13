use super::support::*;

#[tokio::test]
async fn list_model_catalog_enforces_auth_type_policy() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("custom-key"))
        .policy(ProviderPolicy::allow_all().with_allowed_auth_types("custom", [AuthType::ApiKey]))
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
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("auth type is not allowed"))
    );
}

#[tokio::test]
async fn list_model_catalog_reports_auth_type_policy_metadata() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/custom-auth-models")
                .header("authorization", "Bearer custom-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "custom-auth-model", "name": "Custom Auth Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("custom-key"))
        .policy(ProviderPolicy::allow_all().with_allowed_auth_types("custom", [AuthType::Custom]))
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/custom-auth-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert_eq!(
        result.policy.allowed_auth_types,
        Some(vec![AuthType::Custom])
    );
    assert_eq!(result.models[0].id, "custom-auth-model");
}

#[tokio::test]
async fn list_model_catalog_enforces_provider_capability_policy() {
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("custom-key"))
        .policy(
            ProviderPolicy::allow_all().with_required_provider_capabilities(
                "custom",
                ProviderCapabilities {
                    vision: true,
                    ..ProviderCapabilities::default()
                },
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
        matches!(error, ProviderError::PolicyDenied { provider, reason } if provider == "custom" && reason.contains("provider capabilities do not satisfy"))
    );
}

#[tokio::test]
async fn list_model_catalog_reports_provider_capability_policy_metadata() {
    let server = MockServer::start_async().await;
    let catalog = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/agentic-models")
                .header("authorization", "Bearer custom-key");
            then.status(200).json_body(json!({
                "data": [
                    {"id": "agentic-model", "name": "Agentic Model"}
                ]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("custom-key"))
        .policy(
            ProviderPolicy::allow_all().with_required_provider_capabilities(
                "custom",
                ProviderCapabilities {
                    tools: true,
                    reasoning: true,
                    ..ProviderCapabilities::default()
                },
            ),
        )
        .build();

    let result = client
        .list_model_catalog(
            ModelCatalogRequest::builder("custom")
                .models_url(format!("{}/agentic-models", server.base_url()))
                .build(),
        )
        .await
        .unwrap();

    catalog.assert_async().await;
    assert!(result.policy.required_provider_capabilities.tools);
    assert!(!result.policy.required_provider_capabilities.vision);
    assert!(result.policy.required_provider_capabilities.reasoning);
    assert_eq!(result.models[0].id, "agentic-model");
}
