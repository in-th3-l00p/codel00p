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
pub use schema::{AgentSettings, DockerSettings, ResolvedSettings};

#[cfg(test)]
mod tests;
