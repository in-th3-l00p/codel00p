use std::path::PathBuf;

use codel00p_memory::StorageBackedMemoryStore;
use codel00p_protocol::{ProjectRef, SessionId};
use codel00p_session::StorageBackedSessionStore;
use codel00p_storage::{SqliteStorage, StorageScope};

pub type CliResult<T> = Result<T, String>;

#[derive(Clone)]
pub struct CliConfig {
    pub memory_db: PathBuf,
    pub organization_id: String,
    pub project: ProjectRef,
}

/// Optional global flags that override file-based settings for one invocation.
#[derive(Default)]
pub struct GlobalOverrides {
    pub memory_db: Option<PathBuf>,
    pub organization_id: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
}

/// Parse the leading global flags. All are optional now — anything not supplied
/// here falls back to the layered configuration.
pub fn parse_global_overrides(args: Vec<String>) -> CliResult<(GlobalOverrides, Vec<String>)> {
    let mut overrides = GlobalOverrides::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--memory-db" => {
                overrides.memory_db =
                    Some(PathBuf::from(required_value(&args, index, "--memory-db")?));
                index += 2;
            }
            "--organization-id" => {
                overrides.organization_id =
                    Some(required_value(&args, index, "--organization-id")?);
                index += 2;
            }
            "--project-id" => {
                overrides.project_id = Some(required_value(&args, index, "--project-id")?);
                index += 2;
            }
            "--project-name" => {
                overrides.project_name = Some(required_value(&args, index, "--project-name")?);
                index += 2;
            }
            _ => break,
        }
    }

    Ok((overrides, args[index..].to_vec()))
}

/// Resolve the effective `CliConfig` from layered settings plus flag overrides.
pub fn resolve_cli_config(
    resolved: &crate::settings::ResolvedSettings,
    overrides: GlobalOverrides,
) -> CliConfig {
    let memory_db = overrides.memory_db.unwrap_or_else(|| resolved.memory_db());
    let organization_id = overrides
        .organization_id
        .unwrap_or_else(|| resolved.organization_id());
    let project_id = overrides
        .project_id
        .unwrap_or_else(|| resolved.project_id());
    let project_name = overrides
        .project_name
        .unwrap_or_else(|| resolved.project_name());

    CliConfig {
        memory_db,
        organization_id,
        project: ProjectRef::new(project_id, project_name),
    }
}

pub fn required_value(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

pub fn single_id<'a>(args: &'a [String], command: &str) -> CliResult<&'a str> {
    if args.len() != 1 {
        return Err(format!("{command} expects exactly one memory id"));
    }
    Ok(&args[0])
}

pub fn parse_session_id(value: &str) -> CliResult<SessionId> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| format!("invalid --session-id: {error}"))
}

pub fn open_memory_store(
    config: &CliConfig,
) -> CliResult<StorageBackedMemoryStore<codel00p_storage::SqliteStorage>> {
    let storage = SqliteStorage::open(&config.memory_db).map_err(|error| error.to_string())?;
    Ok(StorageBackedMemoryStore::new(
        storage_scope(config),
        storage,
    ))
}

pub fn open_session_store(
    config: &CliConfig,
) -> CliResult<StorageBackedSessionStore<codel00p_storage::SqliteStorage>> {
    let storage = SqliteStorage::open(&config.memory_db).map_err(|error| error.to_string())?;
    Ok(StorageBackedSessionStore::new(
        storage_scope(config),
        storage,
    ))
}

fn storage_scope(config: &CliConfig) -> StorageScope {
    StorageScope::project(&config.organization_id, config.project.id())
}
