mod support;

use codel00p_harness::{
    AgentHarness, ProviderModelClient, SessionId, ToolRegistry, UserMessage, Workspace,
};
use codel00p_providers::{InferenceClient, default_registry};
use support::IntegrationConfig;
use tempfile::tempdir;

#[tokio::test]
#[ignore = "requires CODEL00P_INTEGRATION_TESTS=1, GitHub Copilot credentials, and live API access"]
async fn live_github_copilot_harness_smoke_test() {
    let config = IntegrationConfig::from_env();
    if let Some(message) = config.skip_message("github") {
        eprintln!("{message}");
        return;
    }

    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let provider_client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github", config.require_credential("github"))
        .build();
    let model_client =
        ProviderModelClient::new(provider_client, "github", "gpt-4o-mini-2024-07-18");

    let outcome = AgentHarness::builder()
        .model_client(model_client)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(2)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("live-github-copilot"),
            UserMessage::new("Reply with exactly: codel00p-harness-live-ok"),
        )
        .await
        .expect("live harness request should succeed");

    assert_eq!(
        outcome.assistant_message.as_deref().map(str::trim),
        Some("codel00p-harness-live-ok")
    );
}
