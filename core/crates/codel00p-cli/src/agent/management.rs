//! `agent create | use | list | show | rename | delete` — the local agent
//! registry lifecycle (initiative #13, phase 1).
//!
//! These management subcommands operate on the BASE registry: main.rs skips the
//! `CODEL00P_HOME` override for them, so [`registry::base_home`] resolves the
//! true base regardless of any active agent.

use std::{fs, path::Path};

use crate::config::CliResult;

use super::registry::{self, CreateOptions};

/// Subcommand names that operate on the base registry and must NOT trigger the
/// per-agent `CODEL00P_HOME` override in main.rs.
pub(crate) const MANAGEMENT_SUBCOMMANDS: &[&str] = &[
    "create", "use", "list", "ls", "show", "rename", "delete", "rm",
];

/// Whether `sub` is a base-scoped agent management subcommand.
pub(crate) fn is_management(sub: &str) -> bool {
    MANAGEMENT_SUBCOMMANDS.contains(&sub)
}

/// Dispatch a management subcommand. Returns `Ok(None)` if `command` is not a
/// management verb (so the caller falls through to the run/chat path).
pub(crate) fn run(args: &[String]) -> CliResult<Option<String>> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(None);
    };
    let base = registry::base_home();
    match command.as_str() {
        "create" => create(&base, rest).map(Some),
        "use" => use_agent(&base, rest).map(Some),
        "list" | "ls" => Ok(Some(list(&base))),
        "show" => show(&base, rest).map(Some),
        "rename" => rename(&base, rest).map(Some),
        "delete" | "rm" => delete(&base, rest).map(Some),
        _ => Ok(None),
    }
}

fn create(base: &Path, args: &[String]) -> CliResult<String> {
    let mut name: Option<String> = None;
    let mut opts = CreateOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--description" => {
                opts.description = Some(value(args, index, "--description")?);
                index += 2;
            }
            "--model" => {
                opts.model = Some(value(args, index, "--model")?);
                index += 2;
            }
            "--provider" => {
                opts.provider = Some(value(args, index, "--provider")?);
                index += 2;
            }
            "--from" | "--clone-from" => {
                opts.clone_from = Some(value(args, index, "--from")?);
                index += 2;
            }
            "--persona" => {
                opts.persona = Some(persona_value(&value(args, index, "--persona")?)?);
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag for `agent create`: {other}"));
            }
            other => {
                if name.is_some() {
                    return Err(format!("unexpected argument: {other}"));
                }
                name = Some(other.to_string());
                index += 1;
            }
        }
    }
    let name = name.ok_or_else(|| {
        "usage: codel00p agent create <name> [--description ..] [--model ..] \
         [--provider ..] [--from <agent>] [--persona <text-or-@file>]"
            .to_string()
    })?;

    let info = registry::create_agent(base, &name, &opts)?;
    Ok(format!(
        "Created agent `{}`.\n  home: {}\n  use it:  codel00p agent use {}\n",
        info.name,
        info.home.display(),
        info.name
    ))
}

fn use_agent(base: &Path, args: &[String]) -> CliResult<String> {
    match args.first().map(String::as_str) {
        Some("--default") | Some("-") => {
            registry::set_active_agent(base, None)?;
            Ok("Active agent cleared — using the default (base home).\n".to_string())
        }
        Some(name) => {
            if !registry::agent_exists(base, name) {
                return Err(unknown_agent(base, name));
            }
            registry::set_active_agent(base, Some(name))?;
            Ok(format!("Now using agent `{name}`.\n"))
        }
        None => {
            Err("usage: codel00p agent use <name>  (or `--default` / `-` to clear)".to_string())
        }
    }
}

fn list(base: &Path) -> String {
    let active = registry::active_agent(base);
    let agents = registry::list_agents(base);
    let mut out = String::new();
    // The base home is always present as the implicit default agent.
    let default_marker = if active.is_none() { "*" } else { " " };
    out.push_str(&format!(
        "{default_marker} default  (base home: {})\n",
        base.display()
    ));
    for info in &agents {
        let marker = if active.as_deref() == Some(info.name.as_str()) {
            "*"
        } else {
            " "
        };
        let mut line = format!("{marker} {}", info.name);
        let model = agent_model(&info.home);
        if let Some(desc) = &info.description {
            line.push_str(&format!("  — {desc}"));
        }
        if let Some(model) = model {
            line.push_str(&format!("  [{model}]"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    if agents.is_empty() {
        out.push_str("\nNo agents yet. Create one: codel00p agent create <name>\n");
    }
    out
}

fn show(base: &Path, args: &[String]) -> CliResult<String> {
    let name = args
        .first()
        .ok_or_else(|| "usage: codel00p agent show <name>".to_string())?;
    let info = registry::agent_info(base, name).ok_or_else(|| unknown_agent(base, name))?;
    let active = registry::active_agent(base);
    let mut out = String::new();
    out.push_str(&format!("Agent: {}\n", info.name));
    out.push_str(&format!("  home:        {}\n", info.home.display()));
    if let Some(desc) = &info.description {
        out.push_str(&format!("  description: {desc}\n"));
    }
    if let Some(model) = agent_model(&info.home) {
        out.push_str(&format!("  model:       {model}\n"));
    }
    out.push_str(&format!("  created_at:  {} (epoch ms)\n", info.created_at));
    out.push_str(&format!(
        "  active:      {}\n",
        active.as_deref() == Some(info.name.as_str())
    ));
    Ok(out)
}

fn rename(base: &Path, args: &[String]) -> CliResult<String> {
    let (old, new) = match (args.first(), args.get(1)) {
        (Some(old), Some(new)) => (old, new),
        _ => return Err("usage: codel00p agent rename <old> <new>".to_string()),
    };
    registry::rename_agent(base, old, new)?;
    Ok(format!("Renamed agent `{old}` to `{new}`.\n"))
}

fn delete(base: &Path, args: &[String]) -> CliResult<String> {
    let name = args
        .first()
        .ok_or_else(|| "usage: codel00p agent delete <name>".to_string())?;
    if name == "default" {
        return Err("refusing to delete the default (base home) agent".to_string());
    }
    if !registry::agent_exists(base, name) {
        return Err(unknown_agent(base, name));
    }
    registry::delete_agent(base, name)?;
    Ok(format!("Deleted agent `{name}`.\n"))
}

/// Read `agent.model` from an agent home's `config.toml`, if set, for display.
fn agent_model(home: &Path) -> Option<String> {
    let text = fs::read_to_string(home.join("config.toml")).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    value
        .get("agent")?
        .get("model")?
        .as_str()
        .map(str::to_string)
}

fn unknown_agent(base: &Path, name: &str) -> String {
    let agents = registry::list_agents(base);
    if agents.is_empty() {
        return format!("unknown agent: `{name}` (no agents created yet)");
    }
    let names: Vec<String> = agents.into_iter().map(|info| info.name).collect();
    format!(
        "unknown agent: `{name}`\navailable agents: default, {}",
        names.join(", ")
    )
}

fn value(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

/// A `--persona` value of `@path` reads the persona from a file; anything else
/// is used verbatim.
fn persona_value(raw: &str) -> CliResult<String> {
    if let Some(path) = raw.strip_prefix('@') {
        fs::read_to_string(path).map_err(|e| format!("failed to read persona file {path}: {e}"))
    } else {
        Ok(raw.to_string())
    }
}
