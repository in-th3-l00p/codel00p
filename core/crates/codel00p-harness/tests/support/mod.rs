#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    HarnessError, HarnessInferenceRequest, HarnessInferenceResponse, ModelClient,
};
use codel00p_providers::Credential;

#[derive(Clone, Default)]
pub struct ScriptedModelClient {
    requests: Arc<Mutex<Vec<HarnessInferenceRequest>>>,
    responses: Arc<Mutex<Vec<HarnessInferenceResponse>>>,
}

impl ScriptedModelClient {
    pub fn new(responses: Vec<HarnessInferenceResponse>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses.into_iter().rev().collect())),
        }
    }

    pub fn requests(&self) -> Vec<HarnessInferenceRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

#[async_trait]
impl ModelClient for ScriptedModelClient {
    async fn infer(
        &self,
        request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        self.requests.lock().expect("requests lock").push(request);
        self.responses
            .lock()
            .expect("responses lock")
            .pop()
            .ok_or_else(|| HarnessError::InferenceFailed {
                message: "scripted model client has no response".to_string(),
            })
    }
}

#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    enabled: bool,
}

impl IntegrationConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: env_enabled("CODEL00P_INTEGRATION_TESTS"),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn credential(&self, provider: &str) -> Option<Credential> {
        if !self.enabled {
            return None;
        }

        provider_env_vars(provider)
            .iter()
            .find_map(|key| read_secret(key))
            .map(Credential::api_key)
    }

    pub fn require_credential(&self, provider: &str) -> Credential {
        self.credential(provider).unwrap_or_else(|| {
            panic!(
                "missing integration credential for provider `{provider}`; set CODEL00P_INTEGRATION_TESTS=1 and one of: {}",
                provider_env_vars(provider).join(", ")
            )
        })
    }

    pub fn skip_message(&self, provider: &str) -> Option<String> {
        if !self.enabled() {
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
        "github-models" | "github-model" | "gh-models" => vec![
            "CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
        ],
        "openrouter" | "or" => vec!["CODEL00P_PROVIDER_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"],
        "openai" => vec!["CODEL00P_PROVIDER_OPENAI_API_KEY", "OPENAI_API_KEY"],
        "anthropic" | "claude" => vec![
            "CODEL00P_PROVIDER_ANTHROPIC_API_KEY",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_TOKEN",
        ],
        "azure" | "azure-foundry" => vec![
            "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY",
            "AZURE_FOUNDRY_API_KEY",
        ],
        "gemini" | "google" => vec![
            "CODEL00P_PROVIDER_GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GEMINI_API_KEY",
        ],
        "custom" => vec!["CODEL00P_PROVIDER_CUSTOM_API_KEY"],
        _ => vec![],
    }
}
