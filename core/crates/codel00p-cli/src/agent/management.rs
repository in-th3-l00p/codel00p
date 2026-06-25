//! `agent create | use | list | show | rename | delete` — the local agent
//! registry lifecycle (initiative #13, phase 1).
//!
//! These management subcommands operate on the BASE registry: main.rs skips the
//! `CODEL00P_HOME` override for them, so [`registry::base_home`] resolves the
//! true base regardless of any active agent.

use std::{fs, path::Path};

use crate::config::CliResult;

use super::distribution;
use super::registry::{self, CreateOptions};
use super::routing;

/// Subcommand names that operate on the base registry and must NOT trigger the
/// per-agent `CODEL00P_HOME` override in main.rs.
pub(crate) const MANAGEMENT_SUBCOMMANDS: &[&str] = &[
    "create", "use", "list", "ls", "show", "rename", "delete", "rm", "export", "import", "route",
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
        "export" => export(&base, rest).map(Some),
        "import" => import(&base, rest).map(Some),
        "route" => route(&base, rest).map(Some),
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

/// `agent export <name> [--output <path>]` — package the agent's SHAREABLE files
/// (manifest, config, persona, skills) into a portable `.tar` artifact. Never
/// ships memory, sessions, `.env`, or the active pointer (see `distribution`).
fn export(base: &Path, args: &[String]) -> CliResult<String> {
    let mut name: Option<String> = None;
    let mut output: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                output = Some(value(args, index, "--output")?);
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag for `agent export`: {other}"));
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
    let name =
        name.ok_or_else(|| "usage: codel00p agent export <name> [--output <path>]".to_string())?;
    if !registry::agent_exists(base, &name) {
        return Err(unknown_agent(base, &name));
    }
    let out = output
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| distribution::default_output_path(&name));
    let written = distribution::export_agent(base, &name, &out)?;
    Ok(format!(
        "Exported agent `{name}` to {}\n  shipped:  agent.toml, config.toml, persona.md, skills/ (if present)\n  excluded: memory, sessions, .env, active_agent (private — never packaged)\n  share it: codel00p agent import {}\n",
        written.display(),
        written.display()
    ))
}

/// `agent import <path> [--name <newname>]` — unpack an exported artifact into a
/// NEW agent. Materializes config/persona/skills; private state stays EMPTY
/// (fresh memory/sessions, no creds). Refuses an existing target name.
fn import(base: &Path, args: &[String]) -> CliResult<String> {
    let mut path: Option<String> = None;
    let mut name: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--name" | "-n" => {
                name = Some(value(args, index, "--name")?);
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag for `agent import`: {other}"));
            }
            other => {
                if path.is_some() {
                    return Err(format!("unexpected argument: {other}"));
                }
                path = Some(other.to_string());
                index += 1;
            }
        }
    }
    let path =
        path.ok_or_else(|| "usage: codel00p agent import <path> [--name <newname>]".to_string())?;
    let info = distribution::import_agent(base, Path::new(&path), name.as_deref())?;
    Ok(format!(
        "Imported agent `{}` (fresh memory + sessions).\n  home:   {}\n  use it: codel00p agent use {}\n",
        info.name,
        info.home.display(),
        info.name
    ))
}

/// `agent route <task> [--json] [--limit N]` — rank the registered agents by how
/// well their description/persona matches the task, best first. The top match is
/// the specialist to route the task to. Offline BM25 — explainable, deterministic,
/// no LLM/network. The `--json` form is the seam a delegation/fan-out layer reads.
fn route(base: &Path, args: &[String]) -> CliResult<String> {
    let mut task_parts: Vec<String> = Vec::new();
    let mut json = false;
    let mut limit: Option<usize> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--limit" => {
                let count = value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                limit = Some(count.max(1));
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag for `agent route`: {other}"));
            }
            other => {
                task_parts.push(other.to_string());
                index += 1;
            }
        }
    }
    let task = task_parts.join(" ");
    if task.trim().is_empty() {
        return Err("usage: codel00p agent route <task> [--json] [--limit N]".to_string());
    }
    let agents = registry::list_agents(base);
    if agents.is_empty() {
        return Err(
            "no agents to route to — create one: codel00p agent create <name> --description \"...\""
                .to_string(),
        );
    }

    let mut ranked = routing::rank_agents(&agents, &task);
    // The best match is the top scorer that shares some content with the task
    // (`best_match` is the same seam a delegation/fan-out layer calls to route).
    let best = routing::best_match(&agents, &task, 1).map(|candidate| candidate.name);
    if let Some(count) = limit {
        ranked.truncate(count);
    }

    if json {
        let matches: Vec<serde_json::Value> = ranked
            .iter()
            .map(|candidate| {
                serde_json::json!({
                    "name": candidate.name,
                    "score": candidate.score,
                    "description": candidate.description,
                })
            })
            .collect();
        let payload = serde_json::json!({ "task": task, "best": best, "matches": matches });
        return serde_json::to_string(&payload).map_err(|error| error.to_string());
    }

    let mut out = format!("Routing task: \"{task}\"\n\n");
    for candidate in &ranked {
        let marker = if Some(&candidate.name) == best.as_ref() {
            "\u{2192}"
        } else {
            " "
        };
        let description = candidate.description.as_deref().unwrap_or("");
        out.push_str(&format!(
            "  {marker} {:<16} [{:>3}]  {}\n",
            candidate.name, candidate.score, description
        ));
    }
    out.push('\n');
    match &best {
        Some(name) => out.push_str(&format!(
            "Best match: {name}\n  run it:  codel00p --agent {name} agent run \"{task}\"\n"
        )),
        None => out.push_str(
            "No agent description matched the task — use the default agent or refine agent descriptions.\n",
        ),
    }
    Ok(out)
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
