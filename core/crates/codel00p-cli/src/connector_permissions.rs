use codel00p_harness::PermissionScope;
use codel00p_storage::{KeyValueStore, SqliteStorage, StorageScope, StorageValue};
use serde_json::json;

use crate::config::{CliConfig, CliResult};

const KEY_PREFIX: &str = "connector_permission:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorPermissionStatus {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorPermissionDecision {
    pub tool_name: String,
    pub scope: PermissionScope,
    pub status: ConnectorPermissionStatus,
}

pub fn is_rememberable_permission(tool_name: &str, scope: PermissionScope) -> bool {
    tool_name.starts_with("mcp.") || scope == PermissionScope::ExternalConnector
}

pub fn permission_key(tool_name: &str, scope: PermissionScope) -> String {
    format!("{KEY_PREFIX}{}:{tool_name}", scope_label(scope))
}

pub fn scope_label(scope: PermissionScope) -> &'static str {
    match scope {
        PermissionScope::ReadOnly => "read_only",
        PermissionScope::WorkspaceWrite => "workspace_write",
        PermissionScope::Shell => "shell",
        PermissionScope::Network => "network",
        PermissionScope::ExternalConnector => "external_connector",
        PermissionScope::MemoryWrite => "memory_write",
    }
}

pub fn parse_scope_label(value: &str) -> CliResult<PermissionScope> {
    match value {
        "read_only" | "readonly" | "read-only" => Ok(PermissionScope::ReadOnly),
        "workspace_write" | "workspace-write" | "write" => Ok(PermissionScope::WorkspaceWrite),
        "shell" | "command" => Ok(PermissionScope::Shell),
        "network" => Ok(PermissionScope::Network),
        "external_connector" | "external-connector" | "connector" | "mcp" => {
            Ok(PermissionScope::ExternalConnector)
        }
        "memory_write" | "memory-write" => Ok(PermissionScope::MemoryWrite),
        _ => Err(format!("unknown permission scope: {value}")),
    }
}

pub fn remember_decision(
    config: &CliConfig,
    decision: ConnectorPermissionDecision,
) -> CliResult<()> {
    let mut storage = open_storage(config)?;
    storage
        .put_value(StorageValue::new(
            storage_scope(config),
            permission_key(&decision.tool_name, decision.scope),
            json!({
                "tool_name": decision.tool_name,
                "scope": scope_label(decision.scope),
                "status": status_label(decision.status),
            }),
        ))
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn load_decision(
    config: &CliConfig,
    tool_name: &str,
    scope: PermissionScope,
) -> CliResult<Option<ConnectorPermissionDecision>> {
    let storage = open_storage(config)?;
    let value = storage
        .get_value(&storage_scope(config), &permission_key(tool_name, scope))
        .map_err(|error| error.to_string())?;
    Ok(value.and_then(decode_decision))
}

pub fn list_decisions(config: &CliConfig) -> CliResult<Vec<ConnectorPermissionDecision>> {
    let storage = open_storage(config)?;
    let values = storage
        .list_values(&storage_scope(config), Some(KEY_PREFIX))
        .map_err(|error| error.to_string())?;
    Ok(values.into_iter().filter_map(decode_decision).collect())
}

pub fn forget_decision(
    config: &CliConfig,
    tool_name: &str,
    scope: PermissionScope,
) -> CliResult<bool> {
    let mut storage = open_storage(config)?;
    storage
        .delete_value(&storage_scope(config), &permission_key(tool_name, scope))
        .map_err(|error| error.to_string())
}

pub fn status_label(status: ConnectorPermissionStatus) -> &'static str {
    match status {
        ConnectorPermissionStatus::Allow => "allow",
        ConnectorPermissionStatus::Deny => "deny",
    }
}

fn decode_decision(value: StorageValue) -> Option<ConnectorPermissionDecision> {
    let payload = value.payload();
    let tool_name = payload.get("tool_name")?.as_str()?.to_string();
    let scope = parse_scope_label(payload.get("scope")?.as_str()?).ok()?;
    let status = match payload.get("status")?.as_str()? {
        "allow" => ConnectorPermissionStatus::Allow,
        "deny" => ConnectorPermissionStatus::Deny,
        _ => return None,
    };
    Some(ConnectorPermissionDecision {
        tool_name,
        scope,
        status,
    })
}

fn open_storage(config: &CliConfig) -> CliResult<SqliteStorage> {
    SqliteStorage::open(&config.memory_db).map_err(|error| error.to_string())
}

fn storage_scope(config: &CliConfig) -> StorageScope {
    StorageScope::project(&config.organization_id, config.project.id())
}
