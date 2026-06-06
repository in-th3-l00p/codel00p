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

