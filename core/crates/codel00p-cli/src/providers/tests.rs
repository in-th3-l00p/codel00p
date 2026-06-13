use codel00p_providers::{ChatMessage, InferenceRequest, default_registry};

use super::build_provider_client_with;

fn provider_env_vars(provider: &str) -> Vec<&'static str> {
    default_registry()
        .resolve(provider)
        .map(|profile| profile.env_vars.to_vec())
        .unwrap_or_default()
}

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

        let client = build_provider_client_with(default_registry(), "openai", None).unwrap();
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

#[test]
fn plugin_contributed_provider_is_routable() {
    use std::sync::Arc;

    use codel00p_plugin::{Plugin, PluginRegistry, ProviderProfile};
    use codel00p_providers::{ApiMode, AuthType, OutputTokenParameter, ProviderCapabilities};

    struct ProviderPlugin;
    impl Plugin for ProviderPlugin {
        fn name(&self) -> &str {
            "provider-plugin"
        }

        fn provider_profiles(&self) -> Vec<ProviderProfile> {
            vec![ProviderProfile {
                id: "plugin-openai",
                aliases: &[],
                display_name: "Plugin OpenAI",
                description: "a plugin-contributed provider",
                api_mode: ApiMode::ChatCompletions,
                auth_type: AuthType::ApiKey,
                env_vars: &["CODEL00P_TEST_PLUGIN_PROVIDER_KEY"],
                default_base_url: Some("https://example.test/v1"),
                models_url: None,
                default_aux_model: None,
                output_token_parameter: OutputTokenParameter::MaxTokens,
                capabilities: ProviderCapabilities::agentic(),
            }]
        }
    }

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = "CODEL00P_TEST_PLUGIN_PROVIDER_KEY";
    unsafe {
        std::env::set_var(key, "plugin-secret");
    }

    // Fold the plugin's provider into the built-in registry exactly as the
    // agent run does, then route through it.
    let registry = PluginRegistry::new()
        .register(Arc::new(ProviderPlugin))
        .apply_to_provider_registry(default_registry());
    let client = build_provider_client_with(registry, "plugin-openai", None).unwrap();
    let route = client
        .resolve(
            &InferenceRequest::builder("plugin-openai", "some-model")
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .unwrap();

    unsafe {
        std::env::remove_var(key);
    }

    assert_eq!(
        route.credential_source.as_deref(),
        Some("environment:CODEL00P_TEST_PLUGIN_PROVIDER_KEY")
    );
}
