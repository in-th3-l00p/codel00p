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

pub fn parse_global_args(args: Vec<String>) -> CliResult<(CliConfig, Vec<String>)> {
    let mut memory_db = None;
    let mut organization_id = None;
    let mut project_id = None;
    let mut project_name = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--memory-db" => {
                memory_db = Some(PathBuf::from(required_value(&args, index, "--memory-db")?));
                index += 2;
            }
            "--organization-id" => {
                organization_id = Some(required_value(&args, index, "--organization-id")?);
                index += 2;
            }
            "--project-id" => {
                project_id = Some(required_value(&args, index, "--project-id")?);
                index += 2;
            }
            "--project-name" => {
                project_name = Some(required_value(&args, index, "--project-name")?);
                index += 2;
            }
            _ => break,
        }
    }

    let config = CliConfig {
        memory_db: memory_db.ok_or_else(|| "missing required --memory-db".to_string())?,
        organization_id: organization_id
            .ok_or_else(|| "missing required --organization-id".to_string())?,
        project: ProjectRef::new(
            project_id.ok_or_else(|| "missing required --project-id".to_string())?,
            project_name.ok_or_else(|| "missing required --project-name".to_string())?,
        ),
    };

    Ok((config, args[index..].to_vec()))
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
