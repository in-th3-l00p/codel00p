mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, OpenAI credentials, and live API access"]
async fn live_openai_responses_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("openai") {
        eprintln!("{message}");
        return;
    }

    let model = std::env::var("CODEL00P_PROVIDER_OPENAI_MODEL")
        .unwrap_or_else(|_| "gpt-5-mini".to_string());

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openai", config.require_credential("openai"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("openai", model)
                .message(ChatMessage::user("Reply with exactly: codel00p-openai-ok"))
                .max_output_tokens(32)
                .build(),
        )
        .await
        .expect("live OpenAI Responses request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-openai-ok")
    );
}
