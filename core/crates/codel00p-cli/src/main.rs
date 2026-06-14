use std::{env, path::Path, process::ExitCode};

mod agent;
mod cloud;
mod cloud_client;
mod config;
mod config_cmd;
mod connector_permissions;
mod credentials;
mod cron;
mod error_help;
mod gateway;
mod help;
mod login;
mod mcp_server;
mod memory;
mod plugins;
mod providers;
mod session;
mod settings;
mod skills;
mod tui;

use config::{CliResult, parse_global_overrides, resolve_cli_config};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> CliResult<String> {
    if let Some(help) = help::help_for(&args) {
        return Ok(help.to_string());
    }

    // Seed provider credentials from ~/.codel00p/.env before anything reads them.
    settings::load_env_file();

    let (overrides, rest) = parse_global_overrides(args)?;
    // With no subcommand, `codel00p` opens the interactive chat — the primary UI.
    let (command, rest): (&str, &[String]) = match rest.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("agent", &[]),
    };

    let workspace_start = env::current_dir().map_err(|error| error.to_string())?;

    // Config and capability management operate on settings files directly and do
    // not need a resolved workspace/memory configuration.
    match command {
        "config" => return run_config(&workspace_start, rest),
        "auth" => return run_auth(rest),
        "skills" => return skills::run(&workspace_start, rest),
        _ => {}
    }

    let resolved = settings::load_layered(&workspace_start)?;
    let agent_defaults = resolved.merged.agent.clone();
    let config = resolve_cli_config(&resolved, overrides);

    match command {
        "agent" => agent::run(config, agent_defaults, rest),
        "cron" => cron::run(config, agent_defaults, rest),
        "gateway" => gateway::run(config, agent_defaults, rest),
        "mcp" => mcp_server::run(config, rest),
        "memory" => memory::run(config, rest),
        "cloud" => cloud::run(config, rest),
        "session" => session::run(config, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}

/// `config` groups all configuration: the settings file itself plus the
/// `providers` and `plugins` capability registries.
fn run_config(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    match args.split_first() {
        Some((sub, rest)) if sub == "providers" => providers::run(workspace_start, rest),
        Some((sub, rest)) if sub == "plugins" => plugins::run(workspace_start, rest),
        _ => config_cmd::run(workspace_start, args),
    }
}

/// `auth` groups cloud sign-in and sign-out.
fn run_auth(args: &[String]) -> CliResult<String> {
    match args.split_first() {
        Some((sub, rest)) if sub == "login" => login::run_login(rest),
        Some((sub, rest)) if sub == "logout" => login::run_logout(rest),
        Some((sub, _)) => Err(format!(
            "unknown auth command: {sub}\nUsage: codel00p auth <login|logout>"
        )),
        None => Err("usage: codel00p auth <login|logout>".to_string()),
    }
}
