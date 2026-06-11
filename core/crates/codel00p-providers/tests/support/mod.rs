use codel00p_providers::{AzureManagedIdentitySelector, Credential};

const DEFAULT_AZURE_MANAGED_IDENTITY_RESOURCE: &str = "https://cognitiveservices.azure.com/";
const DEFAULT_AZURE_API_VERSION: &str = "2024-10-21";
const DEFAULT_GITHUB_MODELS_MODEL: &str = "openai/gpt-4o-mini";
const AZURE_MANAGED_IDENTITY_OPT_IN_ENV: &str = "CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_TESTS";
const AWS_MANAGED_IDENTITY_OPT_IN_ENV: &str = "CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_TESTS";
const GCP_MANAGED_IDENTITY_OPT_IN_ENV: &str = "CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_TESTS";
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
const AWS_REGION_ENV_VARS: &[&str] = &[
    "CODEL00P_PROVIDER_AWS_REGION",
    "AWS_REGION",
    "AWS_DEFAULT_REGION",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureManagedIdentityLiveConfig {
    pub resource: String,
    pub selector: AzureManagedIdentitySelector,
    pub identity_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsManagedIdentityLiveConfig {
    pub region: String,
    pub role_name: Option<String>,
    pub identity_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcpManagedIdentityLiveConfig {
    pub service_account: String,
    pub identity_ref: String,
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

    pub fn azure_managed_identity(&self) -> Option<AzureManagedIdentityLiveConfig> {
        if !self.enabled || !env_enabled(AZURE_MANAGED_IDENTITY_OPT_IN_ENV) {
            return None;
        }

        let selector = if let Some(client_id) =
            read_secret("CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_CLIENT_ID")
        {
            AzureManagedIdentitySelector::ClientId(client_id)
        } else if let Some(object_id) =
            read_secret("CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_OBJECT_ID")
        {
            AzureManagedIdentitySelector::ObjectId(object_id)
        } else if let Some(resource_id) =
            read_secret("CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE_ID")
        {
            AzureManagedIdentitySelector::ResourceId(resource_id)
        } else {
            AzureManagedIdentitySelector::SystemAssigned
        };

        Some(AzureManagedIdentityLiveConfig {
            resource: read_secret("CODEL00P_PROVIDER_AZURE_MANAGED_IDENTITY_RESOURCE")
                .unwrap_or_else(|| DEFAULT_AZURE_MANAGED_IDENTITY_RESOURCE.to_string()),
            selector,
            identity_ref: "azure/managed-identity".to_string(),
        })
    }

    pub fn require_azure_managed_identity(&self) -> AzureManagedIdentityLiveConfig {
        self.azure_managed_identity().unwrap_or_else(|| {
            panic!(
                "missing Azure managed identity live integration config; set CODEL00P_INTEGRATION_TESTS=1 and {AZURE_MANAGED_IDENTITY_OPT_IN_ENV}=1"
            )
        })
    }

    pub fn skip_azure_managed_identity_message(&self) -> Option<String> {
        self.skip_managed_identity_opt_in_message("Azure", AZURE_MANAGED_IDENTITY_OPT_IN_ENV)
    }

    pub fn aws_managed_identity(&self) -> Option<AwsManagedIdentityLiveConfig> {
        if !self.enabled || !env_enabled(AWS_MANAGED_IDENTITY_OPT_IN_ENV) {
            return None;
        }

        Some(AwsManagedIdentityLiveConfig {
            region: read_first_secret(AWS_REGION_ENV_VARS)?,
            role_name: read_secret("CODEL00P_PROVIDER_AWS_MANAGED_IDENTITY_ROLE"),
            identity_ref: "aws/instance-profile".to_string(),
        })
    }

    pub fn require_aws_managed_identity(&self) -> AwsManagedIdentityLiveConfig {
        self.aws_managed_identity().unwrap_or_else(|| {
            panic!(
                "missing AWS managed identity live integration config; set CODEL00P_INTEGRATION_TESTS=1, {AWS_MANAGED_IDENTITY_OPT_IN_ENV}=1, and one region variable from: {}",
                AWS_REGION_ENV_VARS.join(", ")
            )
        })
    }

    pub fn skip_aws_managed_identity_message(&self) -> Option<String> {
        if let Some(message) =
            self.skip_managed_identity_opt_in_message("AWS", AWS_MANAGED_IDENTITY_OPT_IN_ENV)
        {
            return Some(message);
        }
        if read_first_secret(AWS_REGION_ENV_VARS).is_none() {
            return Some(format!(
                "skipping live AWS managed identity integration test because no region is configured; set one of: {}",
                AWS_REGION_ENV_VARS.join(", ")
            ));
        }
        None
    }

    pub fn gcp_managed_identity(&self) -> Option<GcpManagedIdentityLiveConfig> {
        if !self.enabled || !env_enabled(GCP_MANAGED_IDENTITY_OPT_IN_ENV) {
            return None;
        }

        let service_account = read_secret("CODEL00P_PROVIDER_GCP_MANAGED_IDENTITY_SERVICE_ACCOUNT")
            .unwrap_or_else(|| "default".to_string());
        let identity_ref = if service_account == "default" {
            "gcp/default-service-account".to_string()
        } else {
            format!("gcp/service-account:{service_account}")
        };

        Some(GcpManagedIdentityLiveConfig {
            service_account,
            identity_ref,
        })
    }

    pub fn require_gcp_managed_identity(&self) -> GcpManagedIdentityLiveConfig {
        self.gcp_managed_identity().unwrap_or_else(|| {
            panic!(
                "missing GCP managed identity live integration config; set CODEL00P_INTEGRATION_TESTS=1 and {GCP_MANAGED_IDENTITY_OPT_IN_ENV}=1"
            )
        })
    }

    pub fn skip_gcp_managed_identity_message(&self) -> Option<String> {
        self.skip_managed_identity_opt_in_message("GCP", GCP_MANAGED_IDENTITY_OPT_IN_ENV)
    }

    fn skip_managed_identity_opt_in_message(
        &self,
        cloud_name: &str,
        opt_in_env: &str,
    ) -> Option<String> {
        if !self.enabled() {
            return Some(
                "skipping live integration test because CODEL00P_INTEGRATION_TESTS is not enabled"
                    .to_string(),
            );
        }
        if !env_enabled(opt_in_env) {
            return Some(format!(
                "skipping live {cloud_name} managed identity integration test because {opt_in_env} is not enabled"
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
    let region = read_first_secret(AWS_REGION_ENV_VARS)?;
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
