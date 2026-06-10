mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, AWS Bedrock credentials, model access, and live API access"]
async fn live_bedrock_converse_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("bedrock") {
        eprintln!("{message}");
        return;
    }

    let model = std::env::var("CODEL00P_PROVIDER_BEDROCK_MODEL")
        .unwrap_or_else(|_| "anthropic.claude-3-5-haiku-20241022-v1:0".to_string());

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("bedrock", config.require_credential("bedrock"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("bedrock", model)
                .message(ChatMessage::user("Reply with exactly: codel00p-bedrock-ok"))
                .max_output_tokens(32)
                .build(),
        )
        .await
        .expect("live Bedrock Converse request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-bedrock-ok")
    );
}
