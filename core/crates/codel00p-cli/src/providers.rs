use codel00p_providers::{InferenceClient, ProviderPolicy, default_registry};

use crate::config::CliResult;

pub fn build_provider_client(
    provider: &str,
    policy_preset: Option<&str>,
) -> CliResult<InferenceClient> {
    let registry = default_registry();
    if registry.resolve(provider).is_none() {
        return Err(format!("unknown provider: {provider}"));
    }

    let policy = policy_preset
        .map(resolve_policy_preset)
        .transpose()?
        .unwrap_or_else(ProviderPolicy::allow_all);

    if registry.credential_from_env(provider).is_none() {
        let env_vars = provider_env_vars(provider);
        return if env_vars.is_empty() {
            Err(format!("missing credential for provider `{provider}`"))
        } else {
            Err(format!(
                "missing credential for provider `{provider}`; set one of: {}",
                env_vars.join(", ")
            ))
        };
    }

    Ok(InferenceClient::builder()
        .registry(registry)
        .policy(policy)
        .credentials_from_env()
        .build())
}

fn resolve_policy_preset(id: &str) -> CliResult<ProviderPolicy> {
    ProviderPolicy::from_preset(id).ok_or_else(|| {
        let available = ProviderPolicy::presets()
            .iter()
            .map(|preset| preset.id)
            .collect::<Vec<_>>()
            .join(", ");
        format!("unknown provider policy preset: {id}; available presets: {available}")
    })
}

pub fn provider_env_vars(provider: &str) -> Vec<&'static str> {
    default_registry()
        .resolve(provider)
        .map(|profile| profile.env_vars.to_vec())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use codel00p_providers::{ChatMessage, InferenceRequest};

    use super::{build_provider_client, provider_env_vars};

    fn with_env_lock(test: impl FnOnce()) {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let keys = ["CODEL00P_PROVIDER_OPENAI_API_KEY", "OPENAI_API_KEY"];
        for key in keys {
            unsafe {
                std::env::remove_var(key);
            }
        }
        test();
        for key in keys {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn github_models_uses_models_specific_token_before_generic_github_tokens() {
        assert_eq!(
            provider_env_vars("github-models"),
            vec![
                "CODEL00P_PROVIDER_GITHUB_MODELS_TOKEN",
                "GITHUB_TOKEN",
                "GH_TOKEN",
            ]
        );
        assert_eq!(
            provider_env_vars("github-model"),
            provider_env_vars("gh-models")
        );
    }

    #[test]
    fn github_keeps_copilot_token_priority() {
        assert_eq!(
            provider_env_vars("github"),
            vec![
                "CODEL00P_PROVIDER_GITHUB_TOKEN",
                "COPILOT_GITHUB_TOKEN",
                "GH_TOKEN",
                "GITHUB_TOKEN",
            ]
        );
    }

    #[test]
    fn build_provider_client_preserves_env_credential_source() {
        with_env_lock(|| {
            unsafe {
                std::env::set_var("CODEL00P_PROVIDER_OPENAI_API_KEY", "env-openai-key");
            }

            let client = build_provider_client("openai", None).unwrap();
            let route = client
                .resolve(
                    &InferenceRequest::builder("openai", "gpt-5-mini")
                        .message(ChatMessage::user("hello"))
                        .build(),
                )
                .unwrap();

            assert_eq!(
                route.credential_source.as_deref(),
                Some("environment:CODEL00P_PROVIDER_OPENAI_API_KEY")
            );
        });
    }
}
