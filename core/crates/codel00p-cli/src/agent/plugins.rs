//! Plugin registry loading for agent runs.

use super::*;

/// Assemble the plugins active for an agent run from layered configuration.
///
/// Enabled ids the catalog does not know are skipped with a warning rather than
/// failing the run, so a stale config entry never bricks the agent.
pub(super) fn load_plugins(workspace: &Path) -> CliResult<PluginRegistry> {
    let resolved = crate::settings::load_layered(workspace)?;
    let catalog = crate::plugins::builtin_catalog();
    let enabled = resolved.merged.plugins.enabled.clone().unwrap_or_default();

    let (known, unknown): (Vec<String>, Vec<String>) =
        enabled.into_iter().partition(|id| catalog.contains(id));
    if !unknown.is_empty() {
        eprintln!(
            "warning: ignoring unknown plugin(s) in config: {}",
            unknown.join(", ")
        );
    }

    catalog.build(&known).map_err(|error| error.to_string())
}
