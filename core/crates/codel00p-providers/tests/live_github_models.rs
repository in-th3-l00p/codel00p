mod support;

use codel00p_providers::{ChatMessage, InferenceClient, InferenceRequest, default_registry};
use support::IntegrationConfig;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, GitHub Models credentials, model access, and live API access"]
async fn live_github_models_chat_completions_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("github-models") {
        eprintln!("{message}");
        return;
    }

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github-models", config.require_credential("github-models"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("github-models", config.github_models_model())
                .message(ChatMessage::user(
                    "Reply with exactly: codel00p-github-models-ok",
                ))
                .max_output_tokens(32)
                .build(),
        )
        .await
        .expect("live GitHub Models request should succeed");

    assert_eq!(
        response.content.as_deref().map(str::trim),
        Some("codel00p-github-models-ok")
    );
}
