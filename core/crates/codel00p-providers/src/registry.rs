use std::collections::BTreeMap;

use crate::profile::{
    ApiMode, AuthType, OutputTokenParameter, ProviderCapabilities, ProviderProfile,
};
use crate::{Credential, CredentialSourceKind, ResolvedProviderCredential};

/// In-memory provider registry with canonical IDs and aliases.
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<&'static str, ProviderProfile>,
    aliases: BTreeMap<&'static str, &'static str>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(mut self, profile: ProviderProfile) -> Self {
        for alias in profile.aliases {
            self.aliases.insert(alias, profile.id);
        }
        self.providers.insert(profile.id, profile);
        self
    }

    pub fn resolve(&self, id_or_alias: &str) -> Option<&ProviderProfile> {
        let normalized = id_or_alias.trim().to_ascii_lowercase();
        let key = normalized.as_str();
        let canonical = self.aliases.get(key).copied().unwrap_or(key);
        self.providers.get(canonical)
    }

    pub fn profiles(&self) -> impl Iterator<Item = &ProviderProfile> {
        self.providers.values()
    }

    pub fn credential_from_env(&self, id_or_alias: &str) -> Option<ResolvedProviderCredential> {
        let profile = self.resolve(id_or_alias)?;
        env_credential_for_profile(profile)
    }
}

/// Built-in provider registry for the first codel00p provider wave.
pub fn default_registry() -> ProviderRegistry {
    ProviderRegistry::new()
        .register(ProviderProfile {
            id: "openai",
            aliases: &["openai-api", "gpt"],
            display_name: "OpenAI",
            description: "OpenAI API",
            api_mode: ApiMode::Responses,
            auth_type: AuthType::ApiKey,
            env_vars: &["CODEL00P_PROVIDER_OPENAI_API_KEY", "OPENAI_API_KEY"],
            default_base_url: Some("https://api.openai.com/v1"),
            models_url: Some("https://api.openai.com/v1/models"),
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxCompletionTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "anthropic",
            aliases: &["claude", "claude-code"],
            display_name: "Anthropic",
            description: "Anthropic Messages API",
            api_mode: ApiMode::AnthropicMessages,
            auth_type: AuthType::ApiKey,
            env_vars: &[
                "CODEL00P_PROVIDER_ANTHROPIC_API_KEY",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_TOKEN",
                "CLAUDE_CODE_OAUTH_TOKEN",
            ],
            default_base_url: Some("https://api.anthropic.com"),
            models_url: Some("https://api.anthropic.com/v1/models"),
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities {
                tools: true,
                streaming: true,
                vision: true,
                reasoning: true,
            },
        })
        .register(ProviderProfile {
            id: "azure-foundry",
            aliases: &["azure", "azure-ai", "azure-ai-foundry"],
            display_name: "Azure AI Foundry",
            description: "Microsoft Azure AI Foundry deployment chat completions endpoint",
            api_mode: ApiMode::AzureChatCompletions,
            auth_type: AuthType::ApiKey,
            env_vars: &[
                "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY",
                "AZURE_FOUNDRY_API_KEY",
            ],
            default_base_url: None,
            models_url: None,
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "bedrock",
            aliases: &["aws", "aws-bedrock", "amazon-bedrock", "amazon"],
            display_name: "AWS Bedrock",
            description: "AWS Bedrock Converse API",
            api_mode: ApiMode::BedrockConverse,
            auth_type: AuthType::AwsSdk,
            env_vars: &[
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
            default_base_url: Some("https://bedrock-runtime.us-east-1.amazonaws.com"),
            models_url: None,
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "gemini",
            aliases: &["google", "google-gemini", "google-ai-studio"],
            display_name: "Google Gemini",
            description: "Google Gemini native and OpenAI-compatible endpoints",
            api_mode: ApiMode::Gemini,
            auth_type: AuthType::ApiKey,
            env_vars: &[
                "CODEL00P_PROVIDER_GEMINI_API_KEY",
                "GOOGLE_API_KEY",
                "GEMINI_API_KEY",
            ],
            default_base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
            models_url: None,
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities {
                tools: true,
                streaming: true,
                vision: true,
                reasoning: true,
            },
        })
        .register(ProviderProfile {
            id: "github",
            aliases: &["github-copilot", "copilot"],
            display_name: "GitHub Copilot",
            description: "GitHub Copilot Chat Completions-compatible endpoint",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::GitHubCopilot,
            env_vars: &[
                "CODEL00P_PROVIDER_GITHUB_TOKEN",
                "COPILOT_GITHUB_TOKEN",
                "GH_TOKEN",
                "GITHUB_TOKEN",
            ],
            default_base_url: Some("https://api.githubcopilot.com"),
            models_url: None,
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxCompletionTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "github-models",
            aliases: &["github-model", "gh-models"],
            display_name: "GitHub Models",
            description: "GitHub Models OpenAI-compatible inference endpoint",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::ApiKey,
            env_vars: &[
                "CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN",
                "GITHUB_TOKEN",
                "GH_TOKEN",
            ],
            default_base_url: Some("https://models.github.ai/inference"),
            models_url: Some("https://models.github.ai/catalog/models"),
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "openrouter",
            aliases: &["or"],
            display_name: "OpenRouter",
            description: "OpenRouter model gateway",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::ApiKey,
            env_vars: &["CODEL00P_PROVIDER_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"],
            default_base_url: Some("https://openrouter.ai/api/v1"),
            models_url: Some("https://openrouter.ai/api/v1/models"),
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "custom",
            aliases: &[
                "ollama",
                "local",
                "vllm",
                "llamacpp",
                "llama.cpp",
                "llama-cpp",
            ],
            display_name: "Custom OpenAI-compatible",
            description: "Configured OpenAI-compatible endpoint",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::Custom,
            env_vars: &["CODEL00P_PROVIDER_CUSTOM_API_KEY"],
            default_base_url: None,
            models_url: None,
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        })
}

fn env_credential_for_profile(profile: &ProviderProfile) -> Option<ResolvedProviderCredential> {
    match profile.auth_type {
        AuthType::ApiKey | AuthType::GitHubCopilot | AuthType::Custom => {
            let (source, secret) = read_first_env_secret(profile.env_vars)?;
            Some(environment_credential(source, Credential::api_key(secret)))
        }
        AuthType::AwsSdk => aws_sigv4_env_credential(),
        AuthType::OAuthExternal | AuthType::CloudProxy => None,
    }
}

fn aws_sigv4_env_credential() -> Option<ResolvedProviderCredential> {
    let (source, access_key_id) =
        read_first_env_secret(&["CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID"])?;
    let (_, secret_access_key) = read_first_env_secret(&[
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "AWS_SECRET_ACCESS_KEY",
    ])?;
    let session_token =
        read_first_env_secret(&["CODEL00P_PROVIDER_AWS_SESSION_TOKEN", "AWS_SESSION_TOKEN"])
            .map(|(_, value)| value);
    let (_, region) = read_first_env_secret(&[
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ])?;

    Some(environment_credential(
        source,
        Credential::aws_sigv4(
            access_key_id,
            secret_access_key,
            session_token.as_deref(),
            region,
        ),
    ))
}

fn environment_credential(
    source_key: impl Into<String>,
    credential: Credential,
) -> ResolvedProviderCredential {
    ResolvedProviderCredential {
        credential,
        source: format!("environment:{}", source_key.into()),
        source_kind: CredentialSourceKind::Environment,
    }
}

fn read_first_env_secret(keys: &[&str]) -> Option<(String, String)> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| ((*key).to_string(), value))
    })
}
