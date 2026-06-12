//! Plugin registry for codel00p.
//!
//! codel00p already exposes several extension seams individually: the harness
//! has a [`Tool`] registry, [`LifecycleHook`]s, and [`AgentEventSink`]s, and the
//! providers crate has a [`ProviderRegistry`] of [`ProviderProfile`]s. Today
//! those seams are assembled by hand at each call site.
//!
//! This crate gives them one narrow waist: a [`Plugin`] contributes any subset
//! of tools, lifecycle hooks, event sinks, and provider profiles, and a
//! [`PluginRegistry`] folds a set of plugins onto base registries. This is the
//! in-process, compile-time half of the
//! [Plugins & Hooks initiative](../../../docs/initiatives/plugins-and-hooks.md):
//! built-in capability and third-party capability flow through the same path,
//! so adding a tool or provider no longer means editing harness internals.
//!
//! Contribution is intentionally additive and order-sensitive. When two plugins
//! contribute a tool or provider profile under the same name, the plugin
//! registered later wins (last-writer-wins), matching the override semantics a
//! user plugin needs to shadow a bundled default.
//!
//! What this crate does **not** do yet: out-of-process / dynamically loaded
//! plugins, config-driven enablement, and hook veto wiring. Those are later
//! phases of the initiative. A plugin here is ordinary Rust compiled into the
//! workspace.

use std::sync::Arc;

pub use codel00p_harness::{AgentEventSink, LifecycleHook, Tool, ToolRegistry};
pub use codel00p_providers::{ProviderProfile, ProviderRegistry};

/// A unit of codel00p capability that can be contributed to the agent runtime.
///
/// Every method has a default empty implementation, so a plugin only overrides
/// the surfaces it actually contributes to. Implementations must be `Send +
/// Sync` because the runtime shares them across asynchronous turns.
pub trait Plugin: Send + Sync {
    /// Stable identifier for this plugin, used for listing and diagnostics.
    fn name(&self) -> &str;

    /// Tools this plugin contributes to the agent's tool registry.
    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        Vec::new()
    }

    /// Turn lifecycle hooks this plugin contributes.
    fn lifecycle_hooks(&self) -> Vec<Arc<dyn LifecycleHook>> {
        Vec::new()
    }

    /// Event sinks this plugin contributes to observe the harness event stream.
    fn event_sinks(&self) -> Vec<Arc<dyn AgentEventSink>> {
        Vec::new()
    }

    /// Provider profiles this plugin contributes to the provider registry.
    ///
    /// Profiles are compile-time data (`&'static` fields), so this surface is
    /// for in-process providers; dynamically configured providers are a later
    /// phase of the initiative.
    fn provider_profiles(&self) -> Vec<ProviderProfile> {
        Vec::new()
    }
}

/// An ordered collection of [`Plugin`]s and the contributions they fold in.
///
/// The registry never executes anything itself; it aggregates contributions and
/// applies them to the base registries the harness already understands. Plugins
/// are applied in registration order, which is what makes overrides
/// deterministic.
#[derive(Clone, Default)]
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin. Later registrations win on name collisions.
    pub fn register(mut self, plugin: Arc<dyn Plugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Names of the registered plugins, in registration order.
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugins
            .iter()
            .map(|plugin| plugin.name().to_string())
            .collect()
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether any plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// All lifecycle hooks contributed by the registered plugins.
    pub fn lifecycle_hooks(&self) -> Vec<Arc<dyn LifecycleHook>> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.lifecycle_hooks())
            .collect()
    }

    /// All event sinks contributed by the registered plugins.
    pub fn event_sinks(&self) -> Vec<Arc<dyn AgentEventSink>> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.event_sinks())
            .collect()
    }

    /// Fold every plugin's tools onto `base`, returning the combined registry.
    ///
    /// Plugin tools are applied in registration order, so a tool name
    /// contributed by a later plugin overrides an earlier one, and any plugin
    /// tool overrides a same-named tool already present in `base`.
    pub fn apply_to_tool_registry(&self, base: ToolRegistry) -> ToolRegistry {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.tools())
            .fold(base, |registry, tool| registry.with_tool_arc(tool))
    }

    /// Fold every plugin's provider profiles onto `base`.
    ///
    /// As with tools, later registrations win: a profile whose `id` matches one
    /// already in `base` (or contributed by an earlier plugin) replaces it.
    pub fn apply_to_provider_registry(&self, base: ProviderRegistry) -> ProviderRegistry {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.provider_profiles())
            .fold(base, |registry, profile| registry.register(profile))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use codel00p_harness::{HarnessError, ToolResult, TurnLifecycleContext, Workspace};
    use codel00p_providers::{
        ApiMode, AuthType, OutputTokenParameter, ProviderCapabilities, ProviderProfile,
    };
    use serde_json::{Value, json};

    // A trivial tool whose execution echoes a marker, so override tests can tell
    // which contribution actually landed in the registry.
    struct MarkerTool {
        name: &'static str,
        marker: &'static str,
    }

    #[async_trait]
    impl Tool for MarkerTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "marker tool"
        }

        fn input_schema(&self) -> Value {
            json!({ "type": "object" })
        }

        async fn execute(
            &self,
            _workspace: &Workspace,
            _input: Value,
        ) -> Result<ToolResult, HarnessError> {
            Ok(ToolResult::json(json!({ "marker": self.marker })))
        }
    }

    struct CountingHook;

    #[async_trait]
    impl LifecycleHook for CountingHook {
        async fn on_turn_started(
            &self,
            _context: TurnLifecycleContext,
        ) -> Result<(), HarnessError> {
            Ok(())
        }
    }

    fn profile(id: &'static str, display_name: &'static str) -> ProviderProfile {
        ProviderProfile {
            id,
            aliases: &[],
            display_name,
            description: "plugin provider",
            api_mode: ApiMode::ChatCompletions,
            auth_type: AuthType::Custom,
            env_vars: &[],
            default_base_url: None,
            models_url: None,
            default_aux_model: None,
            output_token_parameter: OutputTokenParameter::MaxTokens,
            capabilities: ProviderCapabilities::agentic(),
        }
    }

    struct SamplePlugin;

    impl Plugin for SamplePlugin {
        fn name(&self) -> &str {
            "sample"
        }

        fn tools(&self) -> Vec<Arc<dyn Tool>> {
            vec![Arc::new(MarkerTool {
                name: "echo",
                marker: "from-sample",
            })]
        }

        fn lifecycle_hooks(&self) -> Vec<Arc<dyn LifecycleHook>> {
            vec![Arc::new(CountingHook)]
        }

        fn provider_profiles(&self) -> Vec<ProviderProfile> {
            vec![profile("plugin-provider", "Plugin Provider")]
        }
    }

    // A second plugin that shadows the first plugin's tool and provider id.
    struct OverridePlugin;

    impl Plugin for OverridePlugin {
        fn name(&self) -> &str {
            "override"
        }

        fn tools(&self) -> Vec<Arc<dyn Tool>> {
            vec![Arc::new(MarkerTool {
                name: "echo",
                marker: "from-override",
            })]
        }

        fn provider_profiles(&self) -> Vec<ProviderProfile> {
            vec![profile("plugin-provider", "Override Provider")]
        }
    }

    #[test]
    fn empty_registry_contributes_nothing() {
        let registry = PluginRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.plugin_names().is_empty());
        assert!(registry.lifecycle_hooks().is_empty());
        assert!(registry.event_sinks().is_empty());

        let tools = registry.apply_to_tool_registry(ToolRegistry::read_only_defaults());
        assert_eq!(
            tools.names(),
            vec!["list_files", "read_file", "search_text"]
        );
    }

    #[test]
    fn aggregates_contributions() {
        let registry = PluginRegistry::new().register(Arc::new(SamplePlugin));

        assert_eq!(registry.plugin_names(), vec!["sample".to_string()]);
        assert_eq!(registry.lifecycle_hooks().len(), 1);

        let tools = registry.apply_to_tool_registry(ToolRegistry::read_only_defaults());
        assert!(tools.names().contains(&"echo".to_string()));

        let providers = registry.apply_to_provider_registry(ProviderRegistry::new());
        assert_eq!(
            providers.resolve("plugin-provider").map(|p| p.display_name),
            Some("Plugin Provider")
        );
    }

    #[tokio::test]
    async fn later_plugin_overrides_earlier_tool() {
        let registry = PluginRegistry::new()
            .register(Arc::new(SamplePlugin))
            .register(Arc::new(OverridePlugin));

        let tools = registry.apply_to_tool_registry(ToolRegistry::new());

        // Same tool name from two plugins collapses to a single entry...
        assert_eq!(
            tools.names().iter().filter(|name| *name == "echo").count(),
            1
        );

        // ...and the later registration is the one that executes.
        let workspace = Workspace::new(std::env::temp_dir()).expect("workspace");
        let result = tools
            .execute("echo", &workspace, json!({}))
            .await
            .expect("execute echo");
        assert_eq!(result.content(), &json!({ "marker": "from-override" }));
    }

    #[test]
    fn later_plugin_overrides_earlier_provider() {
        let registry = PluginRegistry::new()
            .register(Arc::new(SamplePlugin))
            .register(Arc::new(OverridePlugin));

        let providers = registry.apply_to_provider_registry(ProviderRegistry::new());
        assert_eq!(
            providers.resolve("plugin-provider").map(|p| p.display_name),
            Some("Override Provider")
        );
    }
}
