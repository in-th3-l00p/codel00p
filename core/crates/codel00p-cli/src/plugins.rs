//! Built-in plugin catalog and the `codel00p plugins` command.
//!
//! The catalog is the allow-list of plugins an agent run can enable. Enabling is
//! recorded in layered configuration (`[plugins] enabled`), so it is auditable
//! and layered like every other setting rather than arbitrary code loading.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use codel00p_harness::{HarnessError, PermissionScope, ToolResult, Workspace};
use codel00p_plugin::{Plugin, PluginCatalog, Tool};
use serde_json::{Value, json};

use crate::{config::CliResult, settings};

/// The plugins this build of codel00p ships and can enable by id.
///
/// Today it holds a single reference plugin; future built-ins (and, later,
/// third-party plugins) register here. The id is the stable handle used in
/// `[plugins] enabled` and by `codel00p plugins enable/disable`.
pub fn builtin_catalog() -> PluginCatalog {
    PluginCatalog::new().with(
        "system-info",
        "Adds a read-only system_info tool (host OS/arch + workspace root).",
        || Arc::new(SystemInfoPlugin),
    )
}

/// A small reference plugin: contributes one read-only tool.
struct SystemInfoPlugin;

impl Plugin for SystemInfoPlugin {
    fn name(&self) -> &str {
        "system-info"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![Arc::new(SystemInfoTool)]
    }
}

struct SystemInfoTool;

#[async_trait]
impl Tool for SystemInfoTool {
    fn name(&self) -> &str {
        "system_info"
    }

    fn description(&self) -> &str {
        "Report the host operating system, architecture, and workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        Ok(ToolResult::json(json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
            "workspace_root": workspace.root().display().to_string(),
        })))
    }
}

// --- `codel00p plugins` command -------------------------------------------

pub fn run(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("list", &[][..]),
    };
    match command {
        "list" => plugins_list(workspace_start),
        "enable" => plugins_enable(workspace_start, rest),
        "disable" => plugins_disable(workspace_start, rest),
        _ => Err(format!("unknown plugins command: {command}")),
    }
}

fn plugins_list(workspace_start: &Path) -> CliResult<String> {
    let resolved = settings::load_layered(workspace_start)?;
    let enabled = resolved.merged.plugins.enabled.clone().unwrap_or_default();
    let catalog = builtin_catalog();

    let mut output = String::from("Plugins ([x] = enabled):\n");
    if catalog.entries().is_empty() {
        output.push_str("  (no plugins available)\n");
    }
    for entry in catalog.entries() {
        let mark = if enabled.iter().any(|id| id == entry.id()) {
            "x"
        } else {
            " "
        };
        output.push_str(&format!(
            "  [{mark}] {:<14} {}\n",
            entry.id(),
            entry.description()
        ));
    }

    let unknown: Vec<&String> = enabled.iter().filter(|id| !catalog.contains(id)).collect();
    if !unknown.is_empty() {
        let names = unknown
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("\nEnabled but not installed: {names}\n"));
    }

    output.push_str(
        "\nEnable:  codel00p plugins enable <id>\n\
         Disable: codel00p plugins disable <id>\n",
    );
    Ok(output)
}

struct PluginMutateOptions {
    id: String,
    project: bool,
}

fn parse_plugin_mutate(args: &[String], verb: &str) -> CliResult<PluginMutateOptions> {
    let mut id = None;
    let mut project = false;
    for arg in args {
        match arg.as_str() {
            "--project" => project = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown plugins {verb} option: {flag}"));
            }
            value => {
                if id.is_some() {
                    return Err(format!("unexpected argument: {value}"));
                }
                id = Some(value.to_string());
            }
        }
    }
    Ok(PluginMutateOptions {
        id: id.ok_or_else(|| format!("usage: plugins {verb} <id> [--project]"))?,
        project,
    })
}

fn target_path(workspace_start: &Path, project: bool) -> PathBuf {
    if project {
        settings::project_config_path(workspace_start)
    } else {
        settings::user_config_path()
    }
}

/// The `plugins.enabled` list stored in a single config file (not the merged
/// view), so enable/disable edit exactly the file they target.
fn file_enabled(path: &Path) -> CliResult<Vec<String>> {
    Ok(settings::load_file(path)?
        .and_then(|settings| settings.plugins.enabled)
        .unwrap_or_default())
}

fn write_enabled(path: &Path, enabled: &[String]) -> CliResult<()> {
    if enabled.is_empty() {
        settings::unset_value(path, "plugins.enabled")?;
    } else {
        settings::set_value(path, "plugins.enabled", &enabled.join(","))?;
    }
    Ok(())
}

fn plugins_enable(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let options = parse_plugin_mutate(args, "enable")?;
    let catalog = builtin_catalog();
    if !catalog.contains(&options.id) {
        return Err(format!(
            "unknown plugin: {}; available: {}",
            options.id,
            catalog.ids().join(", ")
        ));
    }

    let path = target_path(workspace_start, options.project);
    let mut enabled = file_enabled(&path)?;
    if enabled.iter().any(|id| id == &options.id) {
        return Ok(format!(
            "Plugin {} already enabled in {}.\n",
            options.id,
            path.display()
        ));
    }
    enabled.push(options.id.clone());
    write_enabled(&path, &enabled)?;
    Ok(format!(
        "Enabled plugin {} in {}.\n",
        options.id,
        path.display()
    ))
}

fn plugins_disable(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let options = parse_plugin_mutate(args, "disable")?;
    let path = target_path(workspace_start, options.project);
    let mut enabled = file_enabled(&path)?;
    let before = enabled.len();
    enabled.retain(|id| id != &options.id);
    if enabled.len() == before {
        return Ok(format!(
            "Plugin {} was not enabled in {}.\n",
            options.id,
            path.display()
        ));
    }
    write_enabled(&path, &enabled)?;
    Ok(format!(
        "Disabled plugin {} in {}.\n",
        options.id,
        path.display()
    ))
}
