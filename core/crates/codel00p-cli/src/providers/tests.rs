use std::collections::HashMap;
use std::path::Path;

use codel00p_providers::{
    ChatMessage, InferenceRequest, ProviderProfile, ProviderRegistry, default_registry,
};

use super::{
    CredentialStore, build_provider_client_with, render_provider_menu, render_setup_summary,
    resolve_model_choice, resolve_preset_id, resolve_provider_id,
};
use crate::config::CliResult;

/// An in-memory credential store for menu-rendering tests.
struct FakeStore(HashMap<String, String>);

impl FakeStore {
    fn with(var: &str) -> Self {
        let mut map = HashMap::new();
        map.insert(var.to_string(), "secret".to_string());
        Self(map)
    }
}

impl CredentialStore for FakeStore {
    fn get(&self, var: &str) -> Option<String> {
        self.0.get(var).cloned()
    }
    fn set(&self, _var: &str, _value: &str) -> CliResult<()> {
        Ok(())
    }
    fn remove(&self, _var: &str) -> CliResult<bool> {
        Ok(false)
    }
}

fn sorted_profiles(registry: &ProviderRegistry) -> Vec<&ProviderProfile> {
    let mut profiles: Vec<&ProviderProfile> = registry.profiles().collect();
    profiles.sort_by_key(|profile| profile.id);
    profiles
}

#[test]
fn resolve_provider_id_accepts_index_id_and_rejects_unknown() {
    let registry = default_registry();
    let profiles = sorted_profiles(&registry);

    // A 1-based index maps to that menu row.
    assert_eq!(
        resolve_provider_id("1", &profiles, &registry),
        Some(profiles[0].id)
    );
    // A canonical id resolves to itself.
    assert_eq!(
        resolve_provider_id("  openai ", &profiles, &registry),
        Some("openai")
    );
    // Out-of-range and unknown both fail.
    assert_eq!(resolve_provider_id("0", &profiles, &registry), None);
    assert_eq!(resolve_provider_id("999", &profiles, &registry), None);
    assert_eq!(resolve_provider_id("nope", &profiles, &registry), None);
}

#[test]
fn resolve_model_choice_handles_index_text_and_blank() {
    let models = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
    assert_eq!(resolve_model_choice("2", &models), Some("beta".to_string()));
    assert_eq!(
        resolve_model_choice(" custom/model ", &models),
        Some("custom/model".to_string())
    );
    assert_eq!(resolve_model_choice("   ", &models), None);
    // An out-of-range number is treated as a free-text id, not an index.
    assert_eq!(resolve_model_choice("99", &models), Some("99".to_string()));
}

#[test]
fn resolve_preset_id_accepts_index_and_id() {
    // The first preset is `allow_all`.
    assert_eq!(resolve_preset_id("1"), Some("allow_all".to_string()));
    assert_eq!(
        resolve_preset_id("allow_all"),
        Some("allow_all".to_string())
    );
    assert_eq!(resolve_preset_id(""), None);
    assert_eq!(resolve_preset_id("not-a-preset"), None);
}

#[test]
fn render_provider_menu_marks_configured_and_default() {
    let registry = default_registry();
    let profiles = sorted_profiles(&registry);
    let openai_var = registry.resolve("openai").expect("openai profile").env_vars[0];
    let store = FakeStore::with(openai_var);

    let menu = render_provider_menu(&profiles, &store, Some("openai"));

    assert!(menu.contains("openai"));
    assert!(menu.contains("[x]")); // openai has a key
    assert!(menu.contains("[ ]")); // some other provider does not
    assert!(menu.contains("(current default)"));
}

#[test]
fn render_setup_summary_includes_only_set_fields() {
    let summary = render_setup_summary(
        "openai",
        Some("gpt-5"),
        None,
        None,
        Path::new("/tmp/config.toml"),
    );
    assert!(summary.contains("provider: openai"));
    assert!(summary.contains("model:    gpt-5"));
    assert!(summary.contains("saved to: /tmp/config.toml"));
    assert!(!summary.contains("base_url"));
    assert!(!summary.contains("preset"));
}

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
