use std::{env, process::ExitCode};

mod agent;
mod config;
mod config_cmd;
mod connector_permissions;
mod help;
mod mcp_server;
mod memory;
mod plugins;
mod providers;
mod session;
mod settings;

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
    let Some((command, rest)) = rest.split_first() else {
        return Err("missing command".to_string());
    };

    let workspace_start = env::current_dir().map_err(|error| error.to_string())?;

    // Config and provider management operate on settings files directly and do
    // not need a resolved workspace/memory configuration.
    match command.as_str() {
        "config" => return config_cmd::run(&workspace_start, rest),
        "providers" => return providers::run(&workspace_start, rest),
        "plugins" => return plugins::run(&workspace_start, rest),
        _ => {}
    }

    let resolved = settings::load_layered(&workspace_start)?;
    let agent_defaults = resolved.merged.agent.clone();
    let config = resolve_cli_config(&resolved, overrides);

    match command.as_str() {
        "agent" => agent::run(config, agent_defaults, rest),
        "mcp" => mcp_server::run(config, rest),
        "memory" => memory::run(config, rest),
        "session" => session::run(config, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}
