//! CLI entrypoints for local memory retrieval and review workflows.
//!
//! The dispatcher stays here; option parsing and formatting live in focused
//! submodules so search/list behavior can evolve independently from review and
//! audit commands.

mod curate;
mod import;
mod json;
mod note;
mod parse;
mod query;
mod review;

use crate::config::{CliConfig, CliResult};

use review::ReviewCommand;

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        // Bare `codel00p memory` on a terminal opens the review dialog; pipes and
        // CI keep the scriptable behavior so output is never corrupted.
        use std::io::IsTerminal;
        return if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
            crate::memory_ui::run(config)
        } else {
            Err("missing memory command".to_string())
        };
    };

    match command.as_str() {
        "search" => query::memory_search(config, rest),
        "retrieve" => query::memory_retrieve(config, rest),
        "similar" => query::memory_similar(config, rest),
        "stale" => query::memory_stale(config, rest),
        "quality" => query::memory_quality(config, rest),
        "curate" => curate::memory_curate(config, rest),
        "list" => query::memory_list(config, rest),
        "show" => query::memory_show(config, rest),
        "note" => note::memory_note(config, rest),
        "audit" => review::memory_audit(config, rest),
        "revisions" => review::memory_revisions(config, rest),
        "approve" => review::memory_review(config, rest, ReviewCommand::Approve),
        "reject" => review::memory_review(config, rest, ReviewCommand::Reject),
        "archive" => review::memory_review(config, rest, ReviewCommand::Archive),
        "edit" => review::memory_edit(config, rest),
        "evidence" => match rest.split_first() {
            Some((sub, evidence_rest)) if sub == "add" => {
                review::memory_evidence_add(config, evidence_rest)
            }
            Some((sub, _)) => Err(format!("unknown memory evidence command: {sub}")),
            None => Err("missing memory evidence command".to_string()),
        },
        "merge" => review::memory_merge(config, rest),
        "split" => review::memory_split(config, rest),
        "restore" => review::memory_restore(config, rest),
        "import" => import::memory_import(config, rest),
        _ => Err(format!("unknown memory command: {command}")),
    }
}
