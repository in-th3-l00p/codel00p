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
    /// How relevance retrieval ranks memory this invocation. Resolved (including
    /// the external-ranker governance gate) in [`resolve_cli_config`] so the
    /// store factory stays a simple match. Defaults to offline BM25.
    pub memory_ranking: MemoryRanking,
}

/// The resolved memory ranking strategy for an invocation. The governance gate
/// is decided once, in [`resolve_cli_config`], so the policy is auditable in one
/// place and [`open_memory_store`] need only switch on the outcome.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MemoryRanking {
    /// Offline BM25 — fully local, the default. No memory content leaves the host.
    #[default]
    Internal,
    /// A remote ranking service at `url`. Only produced once the operator has
    /// opted into the external ranker, set a URL, and enabled the governance gate.
    External { url: String },
}

/// Optional global flags that override file-based settings for one invocation.
#[derive(Debug, Default)]
pub struct GlobalOverrides {
    pub memory_db: Option<PathBuf>,
    pub organization_id: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    /// Select a named agent for this invocation (its home becomes
    /// `CODEL00P_HOME`). Highest precedence over the sticky active pointer.
    pub agent: Option<String>,
}

/// Parse the global flags from anywhere in the argument list, returning the
/// overrides plus the remaining (non-global) tokens — whose first element is the
/// subcommand. All are optional; anything not supplied falls back to the layered
/// configuration.
///
/// These flags are **position-tolerant**: `codel00p agent run --agent coder …`
/// works as well as `codel00p --agent coder agent run …`. This is safe because no
/// subcommand defines a flag named `--agent`/`--memory-db`/`--organization-id`/
/// `--project-id`/`--project-name`, so hoisting them never steals a subcommand
/// flag. A global flag consumes its own value (which must not start with `--`),
/// matching `required_value`; every other token is preserved verbatim and order.
pub fn parse_global_overrides(args: Vec<String>) -> CliResult<(GlobalOverrides, Vec<String>)> {
    let mut overrides = GlobalOverrides::default();
    let mut rest = Vec::new();
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
            "--agent" => {
                overrides.agent = Some(required_value(&args, index, "--agent")?);
                index += 2;
            }
            _ => {
                rest.push(args[index].clone());
                index += 1;
            }
        }
    }

    Ok((overrides, rest))
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

    // Resolve the external-ranker governance gate once. `external_ranking_enabled`
    // is fail-closed: it requires `ranker = "external"`, a non-empty URL, and
    // `allow_external_ranking = true` together, so anything short of a full opt-in
    // keeps retrieval on offline BM25.
    let memory = resolved.memory();
    let memory_ranking = if memory.external_ranking_enabled() {
        MemoryRanking::External {
            url: memory.external_url.clone().unwrap_or_default(),
        }
    } else {
        MemoryRanking::Internal
    };

    CliConfig {
        memory_db,
        organization_id,
        project: ProjectRef::new(project_id, project_name),
        memory_ranking,
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
    let store = StorageBackedMemoryStore::new(storage_scope(config), storage);
    // Inject the external ranker only when the governance gate resolved to it;
    // otherwise the store keeps its default offline BM25 provider.
    let store = match &config.memory_ranking {
        MemoryRanking::Internal => store,
        MemoryRanking::External { url } => store.with_ranker(std::sync::Arc::new(
            crate::memory_ranker::ExternalRanker::new(url.clone()),
        )),
    };
    Ok(store)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn leading_global_flags_are_extracted() {
        let (overrides, rest) =
            parse_global_overrides(owned(&["--agent", "coder", "agent", "run", "hi"])).unwrap();
        assert_eq!(overrides.agent.as_deref(), Some("coder"));
        assert_eq!(rest, owned(&["agent", "run", "hi"]));
    }

    #[test]
    fn trailing_agent_flag_is_extracted_and_command_preserved() {
        // The exact dogfooding papercut: `--agent` after the subcommand.
        let (overrides, rest) =
            parse_global_overrides(owned(&["agent", "run", "hi", "--agent", "coder"])).unwrap();
        assert_eq!(overrides.agent.as_deref(), Some("coder"));
        assert_eq!(rest, owned(&["agent", "run", "hi"]));
    }

    #[test]
    fn global_flags_interleave_with_subcommand_args_preserving_order() {
        let (overrides, rest) = parse_global_overrides(owned(&[
            "memory",
            "--organization-id",
            "org-1",
            "list",
            "--agent",
            "coder",
            "--status",
            "approved",
        ]))
        .unwrap();
        assert_eq!(overrides.agent.as_deref(), Some("coder"));
        assert_eq!(overrides.organization_id.as_deref(), Some("org-1"));
        // Non-global tokens keep their relative order; the command leads.
        assert_eq!(rest, owned(&["memory", "list", "--status", "approved"]));
    }

    #[test]
    fn last_value_wins_for_repeated_flag() {
        let (overrides, rest) =
            parse_global_overrides(owned(&["--agent", "a", "list", "--agent", "b"])).unwrap();
        assert_eq!(overrides.agent.as_deref(), Some("b"));
        assert_eq!(rest, owned(&["list"]));
    }

    #[test]
    fn missing_value_for_global_flag_errors() {
        let error = parse_global_overrides(owned(&["agent", "run", "--agent"])).unwrap_err();
        assert!(error.contains("missing value for --agent"), "got: {error}");
    }

    #[test]
    fn no_global_flags_passes_everything_through() {
        let (overrides, rest) =
            parse_global_overrides(owned(&["skills", "list"])).unwrap();
        assert!(overrides.agent.is_none() && overrides.memory_db.is_none());
        assert_eq!(rest, owned(&["skills", "list"]));
    }

    fn resolved_with_memory(
        memory: crate::settings::schema::MemorySettings,
    ) -> crate::settings::ResolvedSettings {
        crate::settings::ResolvedSettings {
            merged: crate::settings::schema::Settings {
                memory,
                ..Default::default()
            },
            user_path: PathBuf::from("/tmp/codel00p-config.toml"),
            project_path: None,
        }
    }

    #[test]
    fn external_ranker_resolves_only_when_fully_opted_in() {
        let memory = crate::settings::schema::MemorySettings {
            ranker: Some("external".to_string()),
            external_url: Some("https://ranker.internal/rank".to_string()),
            allow_external_ranking: Some(true),
        };
        let config = resolve_cli_config(&resolved_with_memory(memory), GlobalOverrides::default());
        assert_eq!(
            config.memory_ranking,
            MemoryRanking::External {
                url: "https://ranker.internal/rank".to_string()
            }
        );
    }

    #[test]
    fn external_ranker_is_fail_closed_without_the_governance_flag() {
        // Ranker + URL set, but the gate is off → resolves to offline BM25.
        let memory = crate::settings::schema::MemorySettings {
            ranker: Some("external".to_string()),
            external_url: Some("https://ranker.internal/rank".to_string()),
            allow_external_ranking: None,
        };
        let config = resolve_cli_config(&resolved_with_memory(memory), GlobalOverrides::default());
        assert_eq!(config.memory_ranking, MemoryRanking::Internal);
    }

    #[test]
    fn default_memory_settings_resolve_to_internal() {
        let config = resolve_cli_config(
            &resolved_with_memory(crate::settings::schema::MemorySettings::default()),
            GlobalOverrides::default(),
        );
        assert_eq!(config.memory_ranking, MemoryRanking::Internal);
    }
}
