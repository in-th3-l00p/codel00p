use codel00p_providers::Credential;

const DEFAULT_AZURE_API_VERSION: &str = "2024-10-21";
const DEFAULT_GITHUB_MODELS_MODEL: &str = "openai/gpt-4o-mini";
const AZURE_ENDPOINT_ENV_VARS: &[&str] = &[
    "CODEL00P_PROVIDER_AZURE_FOUNDRY_ENDPOINT",
    "AZURE_FOUNDRY_ENDPOINT",
    "AZURE_OPENAI_ENDPOINT",
];
const AZURE_DEPLOYMENT_ENV_VARS: &[&str] = &[
    "CODEL00P_PROVIDER_AZURE_FOUNDRY_DEPLOYMENT",
    "AZURE_FOUNDRY_DEPLOYMENT",
    "AZURE_OPENAI_DEPLOYMENT",
];
const AZURE_API_VERSION_ENV_VARS: &[&str] = &[
    "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_VERSION",
    "AZURE_FOUNDRY_API_VERSION",
    "AZURE_OPENAI_API_VERSION",
];

#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureFoundryLiveConfig {
    pub credential: Credential,
    pub endpoint: String,
    pub deployment: String,
    pub api_version: String,
}

#[allow(dead_code)]
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

    pub fn github_models_model(&self) -> String {
        read_secret("CODEL00P_PROVIDER_GITHUB_MODELS_MODEL")
            .unwrap_or_else(|| DEFAULT_GITHUB_MODELS_MODEL.to_string())
    }

    pub fn azure_foundry(&self) -> Option<AzureFoundryLiveConfig> {
        if !self.enabled {
            return None;
        }

        Some(AzureFoundryLiveConfig {
            credential: self.credential("azure")?,
            endpoint: read_first_secret(AZURE_ENDPOINT_ENV_VARS)?,
            deployment: read_first_secret(AZURE_DEPLOYMENT_ENV_VARS)?,
            api_version: read_first_secret(AZURE_API_VERSION_ENV_VARS)
                .unwrap_or_else(|| DEFAULT_AZURE_API_VERSION.to_string()),
        })
    }

    pub fn require_azure_foundry(&self) -> AzureFoundryLiveConfig {
        self.azure_foundry().unwrap_or_else(|| {
            panic!(
                "missing Azure Foundry live integration config; set CODEL00P_INTEGRATION_TESTS=1, one credential variable from: {}; one endpoint variable from: {}; and one deployment variable from: {}",
                provider_env_vars("azure").join(", "),
                AZURE_ENDPOINT_ENV_VARS.join(", "),
                AZURE_DEPLOYMENT_ENV_VARS.join(", ")
            )
        })
    }

    pub fn skip_azure_foundry_message(&self) -> Option<String> {
        if !self.enabled() {
            return Some(
                "skipping live integration test because CODEL00P_INTEGRATION_TESTS is not enabled"
                    .to_string(),
            );
        }
        if self.credential("azure").is_none() {
            return Some(format!(
                "skipping live Azure Foundry integration test because no credential is configured; set one of: {}",
                provider_env_vars("azure").join(", ")
            ));
        }
        if read_first_secret(AZURE_ENDPOINT_ENV_VARS).is_none() {
            return Some(format!(
                "skipping live Azure Foundry integration test because no endpoint is configured; set one of: {}",
                AZURE_ENDPOINT_ENV_VARS.join(", ")
            ));
        }
        if read_first_secret(AZURE_DEPLOYMENT_ENV_VARS).is_none() {
            return Some(format!(
                "skipping live Azure Foundry integration test because no deployment is configured; set one of: {}",
                AZURE_DEPLOYMENT_ENV_VARS.join(", ")
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
