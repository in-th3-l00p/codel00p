use std::{env, process::ExitCode};

mod agent;
mod config;
mod memory;
mod providers;
mod session;

use config::{CliResult, parse_global_args};

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
    let (config, rest) = parse_global_args(args)?;
    let Some((command, rest)) = rest.split_first() else {
        return Err("missing command".to_string());
    };

    match command.as_str() {
        "agent" => agent::run(config, rest),
        "memory" => memory::run(config, rest),
        "session" => session::run(config, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}
