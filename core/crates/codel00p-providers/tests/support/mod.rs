use codel00p_providers::Credential;

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

        if is_bedrock(provider) {
            return aws_sigv4_credential();
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
        "bedrock" | "aws" | "aws-bedrock" => vec![
            "CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID",
            "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
            "CODEL00P_PROVIDER_AWS_SESSION_TOKEN",
            "CODEL00P_PROVIDER_AWS_REGION",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "AWS_REGION",
            "AWS_DEFAULT_REGION",
        ],
        "custom" => vec!["CODEL00P_PROVIDER_CUSTOM_API_KEY"],
        _ => vec![],
    }
}

fn is_bedrock(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "bedrock" | "aws" | "aws-bedrock"
    )
}

fn aws_sigv4_credential() -> Option<Credential> {
    let access_key_id =
        read_first_secret(&["CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID"])?;
    let secret_access_key = read_first_secret(&[
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "AWS_SECRET_ACCESS_KEY",
    ])?;
    let region = read_first_secret(&[
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ])?;
    let session_token =
        read_first_secret(&["CODEL00P_PROVIDER_AWS_SESSION_TOKEN", "AWS_SESSION_TOKEN"]);

    Some(Credential::aws_sigv4(
        access_key_id,
        secret_access_key,
        session_token.as_deref(),
        region,
    ))
}

fn read_first_secret(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| read_secret(key))
}
