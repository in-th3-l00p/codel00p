use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, default_registry,
};

#[tokio::test]
#[ignore = "requires COPILOT_GITHUB_TOKEN and live GitHub Copilot API access"]
async fn live_github_copilot_chat_completions_smoke_test() {
    let token = std::env::var("COPILOT_GITHUB_TOKEN")
        .expect("COPILOT_GITHUB_TOKEN must be set for the live Copilot smoke test");

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github", Credential::api_key(token))
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
