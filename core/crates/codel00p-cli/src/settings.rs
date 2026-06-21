//! File-based configuration for codel00p.
//!
//! Settings are layered, lowest precedence first:
//! built-in defaults < `~/.codel00p/config.toml` (user) <
//! `./.codel00p/config.toml` (project, discovered by walking up) <
//! `CODEL00P_*` env vars < CLI flags (applied by the caller).
//!
//! Secrets never live here: provider API keys come from `CODEL00P_PROVIDER_*`
//! environment variables, optionally seeded from `~/.codel00p/.env` at startup.

mod edit;
mod loading;
mod paths;
mod schema;

pub use edit::{
    effective_value, migrate, set_value, starter_template, unset_value, write_file_atomic,
};
pub use loading::{load_env_file, load_file, load_layered};
pub use paths::{
    discover_project_config, env_file_path, home_dir, project_config_path, user_config_path,
};
pub use schema::{
    AgentSettings, DockerSettings, ProfileSettings, ResolvedSettings, SshSettings, builtin_profiles,
};

#[cfg(test)]
mod tests;

/// Test-only helpers shared across modules that mutate `CODEL00P_HOME`. Path
/// resolution reads process env, so any test touching the home dir must hold the
/// same lock to avoid racing concurrent tests (e.g. the settings layering tests
/// and the TUI settings-overlay persist test).
#[cfg(test)]
pub(crate) mod test_env {
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs `test` with `CODEL00P_HOME` pointed at `dir`, restoring the prior
    /// value afterward. Serialized via [`lock`].
    pub(crate) fn with_home<T>(dir: &Path, test: impl FnOnce() -> T) -> T {
        let _guard = lock();
        let previous = std::env::var_os("CODEL00P_HOME");
        unsafe { std::env::set_var("CODEL00P_HOME", dir) };
        let result = test();
        unsafe {
            match previous {
                Some(value) => std::env::set_var("CODEL00P_HOME", value),
                None => std::env::remove_var("CODEL00P_HOME"),
            }
        }
        result
    }
}
