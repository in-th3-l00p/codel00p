mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, Azure Foundry credentials, endpoint, deployment, and live API access"]
async fn live_azure_foundry_chat_completions_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_azure_foundry_message() {
        eprintln!("{message}");
        return;
    }

    let azure = config.require_azure_foundry();
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("azure", azure.credential)
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("azure", azure.deployment.clone())
                .base_url(azure.endpoint)
                .deployment(azure.deployment)
                .api_version(azure.api_version)
                .message(ChatMessage::user("Reply with exactly: codel00p-azure-ok"))
                .max_output_tokens(32)
                .build(),
        )
        .await
        .expect("live Azure Foundry request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-azure-ok")
    );
}
