use std::{env, fs, io, path::Path};

use super::paths::{discover_project_config, env_file_path, user_config_path};
use super::schema::{ResolvedSettings, Settings, SettingsResult};

pub fn load_file(path: &Path) -> SettingsResult<Option<Settings>> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str::<Settings>(&text)
            .map(Some)
            .map_err(|error| format!("failed to parse {}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

/// Load and merge settings from all layers (defaults < user < project < env).
pub fn load_layered(workspace_start: &Path) -> SettingsResult<ResolvedSettings> {
    let mut merged = Settings::default();

    let user_path = user_config_path();
    if let Some(user) = load_file(&user_path)? {
        merged.merge(user);
    }

    let project_path = discover_project_config(workspace_start);
    if let Some(path) = &project_path
        && let Some(project) = load_file(path)?
    {
        merged.merge(project);
    }

    apply_env_overrides(&mut merged);

    Ok(ResolvedSettings {
        merged,
        user_path,
        project_path,
    })
}

fn apply_env_overrides(settings: &mut Settings) {
    if let Some(value) = env_string("CODEL00P_ORGANIZATION_ID") {
        settings.workspace.organization_id = Some(value);
    }
    if let Some(value) = env_string("CODEL00P_PROJECT_ID") {
        settings.workspace.project_id = Some(value);
    }
    if let Some(value) = env_string("CODEL00P_PROJECT_NAME") {
        settings.workspace.project_name = Some(value);
    }
    if let Some(value) = env_string("CODEL00P_MEMORY_DB") {
        settings.workspace.memory_db = Some(value);
    }
    if let Some(value) = env_string("CODEL00P_AGENT_PROVIDER") {
        settings.agent.provider = Some(value);
    }
    if let Some(value) = env_string("CODEL00P_AGENT_MODEL") {
        settings.agent.model = Some(value);
    }
}

fn env_string(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

/// Seed process environment from `~/.codel00p/.env`, without overriding any
/// variable already present in the environment.
pub fn load_env_file() {
    let path = env_file_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || env::var_os(key).is_some() {
            continue;
        }
        let value = value.trim().trim_matches('"');
        // SAFETY: called once at startup, before any threads are spawned.
        unsafe {
            env::set_var(key, value);
        }
    }
}
