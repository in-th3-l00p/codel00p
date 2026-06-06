mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, GitHub Copilot credentials, and live API access"]
async fn live_github_copilot_chat_completions_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("github") {
        eprintln!("{message}");
        return;
    }

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github", config.require_credential("github"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("github", "gpt-4o-mini-2024-07-18")
                .message(ChatMessage::user("Reply with exactly: codel00p-copilot-ok"))
                .max_output_tokens(16)
                .build(),
        )
        .await
        .expect("live Copilot request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-copilot-ok")
    );
}
