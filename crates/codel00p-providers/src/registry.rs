use std::collections::BTreeMap;

use crate::profile::{ApiMode, AuthType, ProviderCapabilities, ProviderProfile};

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
            env_vars: &["OPENAI_API_KEY"],
            default_base_url: Some("https://api.openai.com/v1"),
            models_url: Some("https://api.openai.com/v1/models"),
            default_aux_model: None,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "anthropic",
            aliases: &["claude", "claude-code"],
            display_name: "Anthropic",
            description: "Anthropic Messages API",
            api_mode: ApiMode::AnthropicMessages,
            auth_type: AuthType::ApiKey,
            env_vars: &["ANTHROPIC_API_KEY", "ANTHROPIC_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"],
            default_base_url: Some("https://api.anthropic.com"),
            models_url: Some("https://api.anthropic.com/v1/models"),
            default_aux_model: None,
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
            description: "Microsoft Azure AI Foundry OpenAI-compatible endpoint",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::ApiKey,
            env_vars: &["AZURE_FOUNDRY_API_KEY"],
            default_base_url: None,
            models_url: None,
            default_aux_model: None,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "bedrock",
            aliases: &["aws", "aws-bedrock", "amazon-bedrock", "amazon"],
            display_name: "AWS Bedrock",
            description: "AWS Bedrock Converse API",
            api_mode: ApiMode::BedrockConverse,
            auth_type: AuthType::AwsSdk,
            env_vars: &[],
            default_base_url: Some("https://bedrock-runtime.us-east-1.amazonaws.com"),
            models_url: None,
            default_aux_model: None,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "gemini",
            aliases: &["google", "google-gemini", "google-ai-studio"],
            display_name: "Google Gemini",
            description: "Google Gemini native and OpenAI-compatible endpoints",
            api_mode: ApiMode::Gemini,
            auth_type: AuthType::ApiKey,
            env_vars: &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
            default_base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
            models_url: None,
            default_aux_model: None,
            capabilities: ProviderCapabilities {
                tools: true,
                streaming: true,
                vision: true,
                reasoning: true,
            },
        })
        .register(ProviderProfile {
            id: "github",
            aliases: &["github-copilot", "github-models", "github-model", "copilot"],
            display_name: "GitHub Models",
            description: "GitHub Copilot and GitHub Models",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::GitHubCopilot,
            env_vars: &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
            default_base_url: Some("https://api.githubcopilot.com"),
            models_url: None,
            default_aux_model: None,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "openrouter",
            aliases: &["or"],
            display_name: "OpenRouter",
            description: "OpenRouter model gateway",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::ApiKey,
            env_vars: &["OPENROUTER_API_KEY"],
            default_base_url: Some("https://openrouter.ai/api/v1"),
            models_url: Some("https://openrouter.ai/api/v1/models"),
            default_aux_model: None,
            capabilities: ProviderCapabilities::agentic(),
        })
        .register(ProviderProfile {
            id: "custom",
            aliases: &["ollama", "local", "vllm", "llamacpp", "llama.cpp", "llama-cpp"],
            display_name: "Custom OpenAI-compatible",
            description: "Configured OpenAI-compatible endpoint",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::Custom,
            env_vars: &[],
            default_base_url: None,
            models_url: None,
            default_aux_model: None,
            capabilities: ProviderCapabilities::agentic(),
        })
}
