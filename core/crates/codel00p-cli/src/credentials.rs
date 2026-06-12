use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::CliResult;
use crate::settings;

/// Stored cloud credentials written by `codel00p login`, read by the `cloud`
/// commands. Lives at `~/.codel00p/credentials.toml` (or under `$CODEL00P_HOME`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
}

pub fn credentials_path() -> PathBuf {
    settings::home_dir().join("credentials.toml")
}

/// Loads stored credentials, returning the default (all `None`) when the file is
/// absent or unreadable — callers fall back to flags and env vars.
pub fn load() -> Credentials {
    let path = credentials_path();
    let Ok(contents) = fs::read_to_string(&path) else {
        return Credentials::default();
    };
    toml::from_str(&contents).unwrap_or_default()
}

/// Writes credentials with owner-only permissions where supported.
pub fn save(credentials: &Credentials) -> CliResult<()> {
    let path = credentials_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = toml::to_string_pretty(credentials).map_err(|error| error.to_string())?;
    fs::write(&path, contents).map_err(|error| error.to_string())?;
    restrict_permissions(&path);
    Ok(())
}

/// Removes the credentials file. Returns whether one existed.
pub fn clear() -> CliResult<bool> {
    let path = credentials_path();
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}
