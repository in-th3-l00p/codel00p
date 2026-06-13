use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use toml_edit::{DocumentMut, Item, Table, Value};

use super::schema::{CONFIG_VERSION, Settings, SettingsResult};

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
