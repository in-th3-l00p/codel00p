use codel00p_harness::{
    AgentHarness, ProviderModelClient, SessionId, ToolRegistry, UserMessage, Workspace,
};
use codel00p_providers::{InferenceClient, default_registry};
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

#[derive(Debug, Clone)]
struct IntegrationConfig {
    enabled: bool,
}

impl IntegrationConfig {
    fn from_env() -> Self {
        Self {
            enabled: env_enabled("CODEL00P_INTEGRATION_TESTS"),
        }
    }

    fn credential(&self, provider: &str) -> Option<codel00p_providers::Credential> {
        if !self.enabled {
            return None;
        }

        provider_env_vars(provider)
            .iter()
            .find_map(|key| read_secret(key))
            .map(codel00p_providers::Credential::api_key)
    }

    fn require_credential(&self, provider: &str) -> codel00p_providers::Credential {
        self.credential(provider).unwrap_or_else(|| {
            panic!(
                "missing integration credential for provider `{provider}`; set CODEL00P_INTEGRATION_TESTS=1 and one of: {}",
                provider_env_vars(provider).join(", ")
            )
        })
    }

    fn skip_message(&self, provider: &str) -> Option<String> {
        if !self.enabled {
            return Some(
                "skipping live integration test because CODEL00P_INTEGRATION_TESTS is not enabled"
                    .to_string(),
            );
        }
        if self.credential(provider).is_none() {
            return Some(format!(
                "skipping live integration test because provider `{provider}` has no configured credential; set one of: {}",
                provider_env_vars(provider).join(", ")
            ));
        }
        None
    }
}

fn env_enabled(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn read_secret(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn provider_env_vars(provider: &str) -> Vec<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "github" | "github-copilot" | "copilot" => vec![
            "CODEL00P_PROVIDER_GITHUB_TOKEN",
            "COPILOT_GITHUB_TOKEN",
            "GH_TOKEN",
            "GITHUB_TOKEN",
        ],
        _ => vec![],
    }
}
