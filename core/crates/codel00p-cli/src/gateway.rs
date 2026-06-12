//! The `codel00p gateway` command: reach one agent core from chat platforms.
//!
//! `gateway message` is the per-message entrypoint a platform adapter (Slack,
//! Telegram, an HTTP webhook) calls: it maps a conversation to a durable agent
//! session and runs the message as a turn. Network adapters and a long-running
//! server build on this in later slices.

use crate::{
    config::{CliConfig, CliResult},
    settings::AgentSettings,
};

pub fn run(config: CliConfig, defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => return Err("usage: codel00p gateway message ...".to_string()),
    };
    match command {
        "message" => gateway_message(config, &defaults, rest),
        _ => Err(format!("unknown gateway command: {command}")),
    }
}

struct MessageOptions {
    conversation: String,
    user: String,
    text: String,
}

fn parse_message(args: &[String]) -> CliResult<MessageOptions> {
    let mut conversation = None;
    let mut user = None;
    let mut text = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--conversation" => {
                conversation = Some(value_after(args, index, "--conversation")?);
                index += 2;
            }
            "--user" => {
                user = Some(value_after(args, index, "--user")?);
                index += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown gateway message option: {flag}"));
            }
            value => {
                text.push(value.to_string());
                index += 1;
            }
        }
    }

    let usage = "usage: gateway message --conversation <id> --user <id> <text>";
    let text = text.join(" ");
    if text.trim().is_empty() {
        return Err(usage.to_string());
    }
    Ok(MessageOptions {
        conversation: conversation.ok_or(usage)?,
        user: user.ok_or(usage)?,
        text,
    })
}

fn gateway_message(
    config: CliConfig,
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<String> {
    let options = parse_message(args)?;
    crate::agent::run_gateway_message(
        config,
        defaults,
        &options.conversation,
        &options.user,
        &options.text,
    )
}

fn value_after(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}
