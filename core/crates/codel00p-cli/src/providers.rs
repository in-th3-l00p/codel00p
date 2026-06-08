use std::env;

use codel00p_providers::{ApiMode, Credential, InferenceClient, default_registry};

use crate::config::CliResult;

pub fn build_provider_client(provider: &str) -> CliResult<InferenceClient> {
    validate_agent_provider(provider)?;
    let credential = provider_credential(provider).ok_or_else(|| {
        let env_vars = provider_env_vars(provider);
        if env_vars.is_empty() {
            format!("missing credential for provider `{provider}`")
        } else {
            format!(
                "missing credential for provider `{provider}`; set one of: {}",
                env_vars.join(", ")
            )
        }
    })?;

    Ok(InferenceClient::builder()
        .registry(default_registry())
        .credential(provider, credential)
        .build())
}

fn validate_agent_provider(provider: &str) -> CliResult<()> {
    let registry = default_registry();
    let profile = registry
        .resolve(provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    if profile.api_mode != ApiMode::ChatCompletions {
        return Err(format!(
            "provider `{}` uses {}; agent run currently supports chat_completions providers",
            profile.id,
            api_mode_label(profile.api_mode)
        ));
    }
    Ok(())
}

fn api_mode_label(api_mode: ApiMode) -> &'static str {
    match api_mode {
        ApiMode::ChatCompletions => "chat_completions",
        ApiMode::AnthropicMessages => "anthropic_messages",
        ApiMode::Responses => "responses",
        ApiMode::BedrockConverse => "bedrock_converse",
        ApiMode::Gemini => "gemini",
        ApiMode::ExternalProcess => "external_process",
    }
}

pub fn provider_env_vars(provider: &str) -> Vec<&'static str> {
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
        "custom" | "ollama" | "local" | "vllm" | "llamacpp" | "llama.cpp" | "llama-cpp" => {
            vec!["CODEL00P_PROVIDER_CUSTOM_API_KEY"]
        }
        _ => vec![],
    }
}

fn provider_credential(provider: &str) -> Option<Credential> {
    provider_env_vars(provider)
        .iter()
        .find_map(|key| read_secret(key))
        .map(Credential::api_key)
}

fn read_secret(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
