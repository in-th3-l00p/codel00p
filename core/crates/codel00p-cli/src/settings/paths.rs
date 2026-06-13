use std::{
    env,
    path::{Path, PathBuf},
};

const CONFIG_FILE_NAME: &str = "config.toml";
const ENV_FILE_NAME: &str = ".env";
const PROJECT_DIR: &str = ".codel00p";

/// OS home directory (`$HOME` / `%USERPROFILE%`), used for `~` expansion.
fn os_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// codel00p home directory: `$CODEL00P_HOME` if set, else `<os-home>/.codel00p`.
pub fn home_dir() -> PathBuf {
    if let Some(dir) = env::var_os("CODEL00P_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    os_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codel00p")
}

pub fn user_config_path() -> PathBuf {
    home_dir().join(CONFIG_FILE_NAME)
}

pub fn env_file_path() -> PathBuf {
    home_dir().join(ENV_FILE_NAME)
}

pub fn default_memory_db() -> PathBuf {
    home_dir().join("memory.sqlite")
}

/// Expand a leading `~` against the OS home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return os_home().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = os_home()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// Walk up from `start` to the nearest `.codel00p/config.toml`. The user config
/// directory (`~/.codel00p`) is never treated as a project, so a cwd under
/// `$HOME` does not double-load the user config as a project layer.
pub fn discover_project_config(start: &Path) -> Option<PathBuf> {
    let home = home_dir();
    let mut current = start.to_path_buf();
    loop {
        let project_dir = current.join(PROJECT_DIR);
        let candidate = project_dir.join(CONFIG_FILE_NAME);
        if project_dir != home && candidate.is_file() {
            return Some(candidate);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return None,
        }
    }
}

/// Path to a project config under `start` (for writing with `--project`),
/// whether or not it exists yet.
pub fn project_config_path(start: &Path) -> PathBuf {
    discover_project_config(start).unwrap_or_else(|| start.join(PROJECT_DIR).join(CONFIG_FILE_NAME))
}
