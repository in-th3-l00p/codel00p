//! File-based configuration for codel00p.
//!
//! Settings are layered, lowest precedence first:
//! built-in defaults < `~/.codel00p/config.toml` (user) <
//! `./.codel00p/config.toml` (project, discovered by walking up) <
//! `CODEL00P_*` env vars < CLI flags (applied by the caller).
//!
//! Secrets never live here: provider API keys come from `CODEL00P_PROVIDER_*`
//! environment variables, optionally seeded from `~/.codel00p/.env` at startup.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, Value};

pub type SettingsResult<T> = Result<T, String>;

/// Current on-disk schema version. Bump alongside a migration step.
pub const CONFIG_VERSION: u32 = 1;

const CONFIG_FILE_NAME: &str = "config.toml";
const ENV_FILE_NAME: &str = ".env";
const PROJECT_DIR: &str = ".codel00p";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_version: Option<u32>,
    #[serde(skip_serializing_if = "WorkspaceSettings::is_empty")]
    pub workspace: WorkspaceSettings,
    #[serde(skip_serializing_if = "AgentSettings::is_empty")]
    pub agent: AgentSettings,
    #[serde(skip_serializing_if = "PluginSettings::is_empty")]
    pub plugins: PluginSettings,
    #[serde(skip_serializing_if = "DelegationSettings::is_empty")]
    pub delegation: DelegationSettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_db: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_policy_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_sets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remember_permissions: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginSettings {
    /// Ids of catalog plugins enabled for agent runs, in precedence order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<Vec<String>>,
}

impl PluginSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.enabled, other.enabled);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DelegationSettings {
    /// Maximum number of child agents that may run concurrently in a batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_children: Option<u32>,
}

impl DelegationSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(
            &mut self.max_concurrent_children,
            other.max_concurrent_children,
        );
    }
}

impl WorkspaceSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.organization_id, other.organization_id);
        take(&mut self.project_id, other.project_id);
        take(&mut self.project_name, other.project_name);
        take(&mut self.memory_db, other.memory_db);
    }
}

impl AgentSettings {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        take(&mut self.provider, other.provider);
        take(&mut self.model, other.model);
        take(&mut self.base_url, other.base_url);
        take(
            &mut self.provider_policy_preset,
            other.provider_policy_preset,
        );
        take(&mut self.max_iterations, other.max_iterations);
        take(&mut self.permission_mode, other.permission_mode);
        take(&mut self.tool_sets, other.tool_sets);
        take(&mut self.stream, other.stream);
        take(&mut self.remember_permissions, other.remember_permissions);
    }
}

impl Settings {
    fn merge(&mut self, other: Settings) {
        take(&mut self.config_version, other.config_version);
        self.workspace.merge(other.workspace);
        self.agent.merge(other.agent);
        self.plugins.merge(other.plugins);
        self.delegation.merge(other.delegation);
    }
}

fn take<T>(slot: &mut Option<T>, value: Option<T>) {
    if value.is_some() {
        *slot = value;
    }
}

/// The merged settings plus the file paths they came from.
pub struct ResolvedSettings {
    pub merged: Settings,
    pub user_path: PathBuf,
    pub project_path: Option<PathBuf>,
}

impl ResolvedSettings {
    pub fn organization_id(&self) -> String {
        self.merged
            .workspace
            .organization_id
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn project_id(&self) -> String {
        self.merged
            .workspace
            .project_id
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn project_name(&self) -> String {
        self.merged
            .workspace
            .project_name
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    pub fn memory_db(&self) -> PathBuf {
        match &self.merged.workspace.memory_db {
            Some(path) => expand_tilde(path),
            None => default_memory_db(),
        }
    }

    pub fn agent(&self) -> &AgentSettings {
        &self.merged.agent
    }
}

// --- Path resolution -------------------------------------------------------

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

// --- Loading ---------------------------------------------------------------

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

// --- Dotted key access -----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueKind {
    Str,
    Bool,
    U32,
    StrList,
}

/// Every settable dotted key and its value kind. The single source of truth for
/// validation, coercion, and help.
const KEY_SPECS: &[(&str, ValueKind)] = &[
    ("workspace.organization_id", ValueKind::Str),
    ("workspace.project_id", ValueKind::Str),
    ("workspace.project_name", ValueKind::Str),
    ("workspace.memory_db", ValueKind::Str),
    ("agent.provider", ValueKind::Str),
    ("agent.model", ValueKind::Str),
    ("agent.base_url", ValueKind::Str),
    ("agent.provider_policy_preset", ValueKind::Str),
    ("agent.max_iterations", ValueKind::U32),
    ("agent.permission_mode", ValueKind::Str),
    ("agent.tool_sets", ValueKind::StrList),
    ("agent.stream", ValueKind::Bool),
    ("agent.remember_permissions", ValueKind::Bool),
    ("plugins.enabled", ValueKind::StrList),
    ("delegation.max_concurrent_children", ValueKind::U32),
];

pub fn known_keys() -> Vec<&'static str> {
    KEY_SPECS.iter().map(|(key, _)| *key).collect()
}

fn key_kind(key: &str) -> Option<ValueKind> {
    KEY_SPECS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, kind)| *kind)
}

fn unknown_key_error(key: &str) -> String {
    format!(
        "unknown config key: {key}\nvalid keys:\n  {}",
        known_keys().join("\n  ")
    )
}

/// Read the effective value of a dotted key from merged settings.
pub fn effective_value(settings: &Settings, key: &str) -> SettingsResult<Option<String>> {
    if key_kind(key).is_none() {
        return Err(unknown_key_error(key));
    }
    let workspace = &settings.workspace;
    let agent = &settings.agent;
    let value = match key {
        "workspace.organization_id" => workspace.organization_id.clone(),
        "workspace.project_id" => workspace.project_id.clone(),
        "workspace.project_name" => workspace.project_name.clone(),
        "workspace.memory_db" => workspace.memory_db.clone(),
        "agent.provider" => agent.provider.clone(),
        "agent.model" => agent.model.clone(),
        "agent.base_url" => agent.base_url.clone(),
        "agent.provider_policy_preset" => agent.provider_policy_preset.clone(),
        "agent.max_iterations" => agent.max_iterations.map(|value| value.to_string()),
        "agent.permission_mode" => agent.permission_mode.clone(),
        "agent.tool_sets" => agent.tool_sets.as_ref().map(|sets| sets.join(",")),
        "agent.stream" => agent.stream.map(|value| value.to_string()),
        "agent.remember_permissions" => agent.remember_permissions.map(|value| value.to_string()),
        "plugins.enabled" => settings.plugins.enabled.as_ref().map(|sets| sets.join(",")),
        "delegation.max_concurrent_children" => settings
            .delegation
            .max_concurrent_children
            .map(|value| value.to_string()),
        _ => None,
    };
    Ok(value)
}

fn read_document(path: &Path) -> SettingsResult<DocumentMut> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .parse::<DocumentMut>()
            .map_err(|error| format!("failed to parse {}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn coerce(kind: ValueKind, raw: &str) -> SettingsResult<Value> {
    match kind {
        ValueKind::Str => Ok(Value::from(raw)),
        ValueKind::Bool => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Ok(Value::from(true)),
            "false" | "no" | "off" | "0" => Ok(Value::from(false)),
            _ => Err(format!("expected a boolean (true/false), got `{raw}`")),
        },
        ValueKind::U32 => raw
            .trim()
            .parse::<u32>()
            .map(|value| Value::from(value as i64))
            .map_err(|_| format!("expected a non-negative integer, got `{raw}`")),
        ValueKind::StrList => {
            let mut array = toml_edit::Array::new();
            for item in raw.split(',') {
                let item = item.trim();
                if !item.is_empty() {
                    array.push(item);
                }
            }
            Ok(Value::Array(array))
        }
    }
}

/// Set a dotted key in the config document at `path` (created if absent),
/// stamping `config_version`, backing up, and writing atomically.
pub fn set_value(path: &Path, key: &str, raw: &str) -> SettingsResult<()> {
    let kind = key_kind(key).ok_or_else(|| unknown_key_error(key))?;
    let (section, field) = key.split_once('.').expect("validated keys contain a dot");
    let value = coerce(kind, raw)?;

    let mut doc = read_document(path)?;
    ensure_version(&mut doc);
    let table = section_table(&mut doc, section)?;
    table.insert(field, Item::Value(value));

    write_document(path, &doc)
}

/// Remove a dotted key. Returns whether anything was removed.
pub fn unset_value(path: &Path, key: &str) -> SettingsResult<bool> {
    if key_kind(key).is_none() {
        return Err(unknown_key_error(key));
    }
    let (section, field) = key.split_once('.').expect("validated keys contain a dot");

    let mut doc = read_document(path)?;
    let removed = match doc.get_mut(section).and_then(Item::as_table_mut) {
        Some(table) => {
            let removed = table.remove(field).is_some();
            if table.is_empty() {
                doc.remove(section);
            }
            removed
        }
        None => false,
    };

    if removed {
        ensure_version(&mut doc);
        write_document(path, &doc)?;
    }
    Ok(removed)
}

fn ensure_version(doc: &mut DocumentMut) {
    if doc.get("config_version").is_none() {
        doc["config_version"] = Item::Value(Value::from(CONFIG_VERSION as i64));
    }
}

/// Bring a config file up to the current schema version, writing it back if the
/// file changed. v1 has no migration steps yet beyond stamping the version.
pub fn migrate(path: &Path) -> SettingsResult<u32> {
    let mut doc = read_document(path)?;
    let before = doc.to_string();
    ensure_version(&mut doc);
    if doc.to_string() != before {
        write_document(path, &doc)?;
    }
    Ok(CONFIG_VERSION)
}

fn section_table<'a>(doc: &'a mut DocumentMut, section: &str) -> SettingsResult<&'a mut Table> {
    let entry = doc
        .entry(section)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("config key `{section}` is not a table"))?;
    Ok(entry)
}

fn write_document(path: &Path, doc: &DocumentMut) -> SettingsResult<()> {
    write_file_atomic(path, &doc.to_string())
}

/// Write `contents` to `path` atomically, backing up any existing file.
pub fn write_file_atomic(path: &Path, contents: &str) -> SettingsResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    if path.exists() {
        let _ = fs::copy(path, backup_path(path));
    }
    let tmp = tmp_path(path);
    fs::write(&tmp, contents)
        .map_err(|error| format!("failed to write {}: {error}", tmp.display()))?;
    fs::rename(&tmp, path)
        .map_err(|error| format!("failed to replace {}: {error}", path.display()))?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bak.{stamp}"));
    path.with_file_name(name)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// A commented starter config written by `config init`.
pub fn starter_template() -> String {
    format!(
        "# codel00p configuration. Edit with `codel00p config` or by hand.\n\
         # Precedence: defaults < this file < ./.codel00p/config.toml < env < CLI flags.\n\
         config_version = {CONFIG_VERSION}\n\
         \n\
         [workspace]\n\
         # organization_id = \"default\"\n\
         # project_id = \"default\"\n\
         # project_name = \"default\"\n\
         # memory_db = \"~/.codel00p/memory.sqlite\"\n\
         \n\
         [agent]\n\
         # provider = \"openrouter\"\n\
         # model = \"openai/gpt-4o-mini\"\n\
         # stream = true\n\
         # permission_mode = \"ask\"   # allow | ask | deny\n\
         # tool_sets = [\"read\"]       # read | edit | command | git | all\n"
    )
}

#[cfg(test)]
mod tests;
