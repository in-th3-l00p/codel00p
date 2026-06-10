mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, Gemini credentials, and live API access"]
async fn live_gemini_generate_content_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("gemini") {
        eprintln!("{message}");
        return;
    }

    let model = std::env::var("CODEL00P_PROVIDER_GEMINI_MODEL")
        .unwrap_or_else(|_| "gemini-2.5-flash".to_string());

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("gemini", config.require_credential("gemini"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("gemini", model)
                .message(ChatMessage::user("Reply with exactly: codel00p-gemini-ok"))
                .max_output_tokens(32)
                .build(),
        )
        .await
        .expect("live Gemini GenerateContent request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-gemini-ok")
    );
}
