use std::{env, path::Path, process::ExitCode};

mod actor;
mod agent;
mod cloud;
mod cloud_client;
mod config;
mod config_cmd;
mod config_ui;
mod connector_permissions;
mod credentials;
mod cron;
mod cron_ui;
mod dialog;
mod error_help;
mod gateway;
mod help;
mod login;
mod mcp_server;
mod memory;
mod memory_ui;
mod plugins;
mod providers;
mod session;
mod sessions_ui;
mod settings;
mod skills;
mod skills_ui;
mod tui;
mod uninstall;
mod update;

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
    // `--version` / `-V` / `version` report the build and exit before anything else.
    if matches!(
        args.first().map(String::as_str),
        Some("--version" | "-V" | "version")
    ) {
        return Ok(format!("codel00p {}\n", update::current_version()));
    }

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

    // `update` manages itself, and `uninstall` is about to remove the binary, so
    // neither nudges; every other command refreshes the update cache in the
    // background and nudges (once) if a newer release is already known.
    if command != "update" && command != "uninstall" {
        update::spawn_background_check();
        if let Some(notice) = update::startup_notice() {
            eprintln!("{notice}\n");
        }
    }

    let workspace_start = env::current_dir().map_err(|error| error.to_string())?;

    // Config and capability management operate on settings files directly and do
    // not need a resolved workspace/memory configuration.
    match command {
        "config" => return run_config(&workspace_start, rest),
        "auth" => return run_auth(rest),
        "skills" => return skills::run(&workspace_start, rest),
        "update" => return update::run(rest),
        "uninstall" => return uninstall::run(rest),
        _ => {}
    }

    // First-run onboarding: with nothing configured yet, walk an interactive user
    // through setup before running their command. Non-interactive shells skip it.
    if interactive() && needs_setup(&workspace_start) {
        let summary = config_ui::run(&workspace_start, config_ui::Section::Menu)?;
        eprint!("{summary}");
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
        "session" | "sessions" => session::run(config, agent_defaults, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}

/// `config` groups all configuration: the settings file itself plus the
/// `providers` and `plugins` capability registries.
fn run_config(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    match args.split_first() {
        // `config providers` with no subcommand opens the dialog at the Providers
        // section; with a subcommand it stays the scriptable CLI.
        Some((sub, rest)) if sub == "providers" => {
            if rest.is_empty() && interactive() {
                config_ui::run(workspace_start, config_ui::Section::Providers)
            } else {
                providers::run(workspace_start, rest)
            }
        }
        Some((sub, rest)) if sub == "plugins" => plugins::run(workspace_start, rest),
        // Bare `config` opens the full configuration dialog when interactive.
        None if interactive() => config_ui::run(workspace_start, config_ui::Section::Menu),
        _ => config_cmd::run(workspace_start, args),
    }
}

/// Whether both stdin and stdout are a terminal, so an interactive dialog is safe
/// (pipes, CI, and `--json`-style automation fall back to the scriptable paths).
fn interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// The first-run condition: no provider has been configured yet.
fn needs_setup(workspace_start: &Path) -> bool {
    settings::load_layered(workspace_start)
        .ok()
        .and_then(|resolved| {
            settings::effective_value(&resolved.merged, "agent.provider")
                .ok()
                .flatten()
        })
        .is_none()
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
