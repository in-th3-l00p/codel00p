mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, Anthropic credentials, and live API access"]
async fn live_anthropic_messages_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("anthropic") {
        eprintln!("{message}");
        return;
    }

    let model = std::env::var("CODEL00P_PROVIDER_ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-3-5-haiku-20241022".to_string());

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("anthropic", config.require_credential("anthropic"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("anthropic", model)
                .message(ChatMessage::user(
                    "Reply with exactly: codel00p-anthropic-ok",
                ))
                .max_output_tokens(32)
                .build(),
        )
        .await
        .expect("live Anthropic request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-anthropic-ok")
    );
}
