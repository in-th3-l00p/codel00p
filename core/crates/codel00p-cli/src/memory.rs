//! CLI entrypoints for local memory retrieval and review workflows.
//!
//! The dispatcher stays here; option parsing and formatting live in focused
//! submodules so search/list behavior can evolve independently from review and
//! audit commands.

mod json;
mod parse;
mod query;
mod review;

use crate::config::{CliConfig, CliResult};

use review::ReviewCommand;

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing memory command".to_string());
    };

    match command.as_str() {
        "search" => query::memory_search(config, rest),
        "similar" => query::memory_similar(config, rest),
        "stale" => query::memory_stale(config, rest),
        "quality" => query::memory_quality(config, rest),
        "list" => query::memory_list(config, rest),
        "show" => query::memory_show(config, rest),
        "audit" => review::memory_audit(config, rest),
        "approve" => review::memory_review(config, rest, ReviewCommand::Approve),
        "reject" => review::memory_review(config, rest, ReviewCommand::Reject),
        "archive" => review::memory_review(config, rest, ReviewCommand::Archive),
        "edit" => review::memory_edit(config, rest),
        "merge" => review::memory_merge(config, rest),
        "restore" => review::memory_restore(config, rest),
        _ => Err(format!("unknown memory command: {command}")),
    }
}
