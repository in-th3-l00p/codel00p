//! Local multi-agent personas (#13) support for the TUI: building the agent
//! switcher list and the live-switch that re-points the running TUI at another
//! agent's home (memory + sessions + config).
//!
//! The decisive primitive (Option A in the initiative): an agent is a
//! `CODEL00P_HOME`-scoped directory. The base home is the implicit `default`
//! agent. Switching = pointing `CODEL00P_HOME` at the target home and rebuilding
//! the layered config — memory (`<home>/memory.sqlite`) and sessions (same DB)
//! isolate automatically from the home boundary.

use std::path::{Path, PathBuf};

use crate::agent::registry;
use crate::config::{CliConfig, GlobalOverrides, resolve_cli_config};

use super::overlay::AgentChoice;

/// The name shown for the base home (the implicit default agent).
pub(crate) const DEFAULT_AGENT_LABEL: &str = "default";

/// Resolves the TRUE base home for the running process. `main` stashes it in
/// `CODEL00P_BASE_HOME` before it overrides `CODEL00P_HOME` for the active agent,
/// because `registry::base_home()` reads `CODEL00P_HOME` live and would return an
/// agent's home (not the base) after a switch. Falls back to `base_home()` when
/// the stash is absent (e.g. tests that don't go through `main`, before any
/// switch has mutated the env).
pub(crate) fn base_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CODEL00P_BASE_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    registry::base_home()
}

/// Builds the agent switcher list: the `default` (base home) agent first, then
/// every registry agent, sorted by name. The `active` flag marks the agent the
/// TUI is currently pointed at (`active_name == None` means the default home).
///
/// `base` is the captured true base home (see [`base_home`]). Runs a blocking
/// registry scan, so callers invoke it off the UI task.
pub(crate) fn list_agent_choices(base: &Path, active_name: Option<&str>) -> Vec<AgentChoice> {
    let mut choices = vec![AgentChoice {
        name: DEFAULT_AGENT_LABEL.to_string(),
        description: Some("the base home — no persona override".to_string()),
        active: active_name.is_none(),
    }];
    for info in registry::list_agents(base) {
        let active = active_name == Some(info.name.as_str());
        choices.push(AgentChoice {
            name: info.name,
            description: info.description,
            active,
        });
    }
    choices
}

/// Resolves a `CliConfig` for an explicit agent home. Rebuilds the layered
/// settings (`load_layered`) and the derived `memory_db` so the next harness
/// build reads the agent's own `<home>/memory.sqlite`, config, and sessions.
///
/// `load_layered` resolves the user config + `memory_db` from the global
/// `CODEL00P_HOME` via `home_dir()`, so the caller MUST have set
/// `CODEL00P_HOME` to `home` before calling this. We keep that env mutation and
/// this reload together in [`switch_config`] for the live-switch path; this
/// function is split out so it is independently testable with an explicit home.
pub(crate) fn config_for_home(workspace_start: &Path) -> Result<CliConfig, String> {
    let resolved = crate::settings::load_layered(workspace_start)?;
    // No global flag overrides on a live switch: the agent's home + its layered
    // config are the source of truth (matching `main`'s startup resolution).
    Ok(resolve_cli_config(&resolved, GlobalOverrides::default()))
}

/// Performs the live agent switch at the config layer and returns the rebuilt
/// `CliConfig` for the target agent.
///
/// Steps (all on the UI task, only when idle so no harness is in flight):
/// 1. Persist the sticky active pointer (`registry::set_active_agent`).
/// 2. Re-point the process `CODEL00P_HOME` at the target home (or unset it back
///    to the base for `default`).
/// 3. Rebuild the layered config so `memory_db` points at the target's
///    `memory.sqlite` — the live memory switch.
///
/// `name == None` (or the `default` label) switches back to the base home.
///
/// # Thread-safety
/// The `env::set_var("CODEL00P_HOME", ..)` mirrors `main`'s startup override.
/// It runs on the single UI task between turns (guarded by `!app.turn.running`),
/// so no worker thread is reading the home concurrently — the only readers are
/// the synchronous `load_layered` call right after, and the next `spawn_turn`
/// which clones the already-rebuilt `CliConfig`.
pub(crate) fn switch_config(
    base: &Path,
    name: Option<&str>,
    workspace_start: &Path,
) -> Result<(CliConfig, Option<String>), String> {
    // Normalize the `default` label to "no agent" (base home).
    let target = match name {
        Some(n) if n == DEFAULT_AGENT_LABEL => None,
        other => other,
    };

    // 1. Persist the sticky pointer (None clears it back to the default).
    registry::set_active_agent(base, target)?;

    // 2. Re-point CODEL00P_HOME at the new home (or the base for default).
    let home = match target {
        Some(n) => {
            registry::validate_name(n)?;
            let home = registry::agent_home(base, n);
            std::fs::create_dir_all(&home)
                .map_err(|e| format!("failed to create agent home {}: {e}", home.display()))?;
            home
        }
        None => base.to_path_buf(),
    };
    // SAFETY: see the function doc — single UI task, between turns, no harness in
    // flight; mirrors `main`'s startup `set_var`.
    unsafe { std::env::set_var("CODEL00P_HOME", &home) };

    // 3. Rebuild the layered config against the new home (live memory switch).
    let config = config_for_home(workspace_start)?;
    Ok((config, target.map(str::to_string)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn list_includes_default_and_marks_active() {
        let tmp = tempfile::tempdir().unwrap();
        // `with_home` isolates CODEL00P_HOME under the shared env lock so the
        // registry scan + this test don't race other env-touching tests.
        crate::settings::test_env::with_home(tmp.path(), || {
            let base = tmp.path().to_path_buf();

            // No agents yet: only the default, active when active_name is None.
            let choices = list_agent_choices(&base, None);
            assert_eq!(choices.len(), 1);
            assert_eq!(choices[0].name, DEFAULT_AGENT_LABEL);
            assert!(choices[0].active);

            // Create two agents; they appear, sorted, with the active one marked.
            registry::create_agent(&base, "scout", &registry::CreateOptions::default()).unwrap();
            registry::create_agent(&base, "writer", &registry::CreateOptions::default()).unwrap();
            let choices = list_agent_choices(&base, Some("scout"));
            let names: Vec<_> = choices.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, vec![DEFAULT_AGENT_LABEL, "scout", "writer"]);
            assert!(!choices[0].active); // default not active
            assert!(choices[1].active); // scout active
            assert!(!choices[2].active);
        });
    }

    #[test]
    fn switch_repoints_home_and_memory_db() {
        let tmp = tempfile::tempdir().unwrap();
        crate::settings::test_env::with_home(tmp.path(), || {
            let base = tmp.path().to_path_buf();
            registry::create_agent(&base, "scout", &registry::CreateOptions::default()).unwrap();

            let workspace = tmp.path().join("workspace");
            fs::create_dir_all(&workspace).unwrap();

            // Switch to scout: config.memory_db must live under scout's home —
            // this is the live memory-switch proof at the config layer.
            let (config, active) = switch_config(&base, Some("scout"), &workspace).unwrap();
            assert_eq!(active.as_deref(), Some("scout"));
            let scout_home = registry::agent_home(&base, "scout");
            assert_eq!(config.memory_db, scout_home.join("memory.sqlite"));
            // Sticky pointer persisted in the base home.
            assert_eq!(registry::active_agent(&base).as_deref(), Some("scout"));

            // Switch back to default: memory_db returns to the base home and the
            // sticky pointer is cleared. Crucially we pass the CAPTURED base, not
            // a re-read of `base_home()` (which would now see scout's home).
            let (config, active) =
                switch_config(&base, Some(DEFAULT_AGENT_LABEL), &workspace).unwrap();
            assert_eq!(active, None);
            assert_eq!(config.memory_db, base.join("memory.sqlite"));
            assert_eq!(registry::active_agent(&base), None);
        });
    }
}
