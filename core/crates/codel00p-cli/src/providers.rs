use std::env;

use codel00p_providers::{Credential, InferenceClient, default_registry};

use crate::config::CliResult;

pub fn build_provider_client(provider: &str) -> CliResult<InferenceClient> {
    let registry = default_registry();
    if registry.resolve(provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }

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
        .registry(registry)
        .credential(provider, credential)
        .build())
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
        "custom" | "ollama" | "local" | "vllm" | "llamacpp" | "llama.cpp" | "llama-cpp" => {
            vec!["CODEL00P_PROVIDER_CUSTOM_API_KEY"]
        }
        _ => vec![],
    }
}

fn provider_credential(provider: &str) -> Option<Credential> {
    if is_bedrock(provider) {
        return aws_sigv4_credential();
    }

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
