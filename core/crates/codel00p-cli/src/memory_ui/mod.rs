//! The full-screen `codel00p memory` review dialog.
//!
//! Bare `codel00p memory` opens this on a terminal; the scriptable subcommands
//! (`list`, `approve`, ...) remain for non-TTY use. The pure model and rendering
//! live in [`model`] and [`view`]; the terminal lifecycle and loop are shared via
//! [`crate::dialog`], and the closure here performs the repository effects an
//! update asks for.

mod model;
#[cfg(test)]
mod tests;
mod view;

use codel00p_memory::{
    MemoryEdit, MemoryListFilter, MemoryRecord, MemoryRepository, ReviewDecision,
};

use crate::config::{CliConfig, CliResult, open_memory_store};
use model::{AuditRow, Flow, MemoryModel, MemoryRow, Mutation, Screen};

type Store = codel00p_memory::StorageBackedMemoryStore<codel00p_storage::SqliteStorage>;

/// Runs the review dialog. The terminal lifecycle and blocking loop come from
/// [`crate::dialog`]; the `on_key` closure performs the repository effects.
pub(crate) fn run(config: CliConfig) -> CliResult<String> {
    let mut store = open_memory_store(&config)?;
    let mut model = MemoryModel::new(crate::actor::infer_actor());
    reload(&store, &config, &mut model)?;

    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Stay => {}
            Flow::Quit => return Ok(false),
            Flow::Reload => reload(&store, &config, model)?,
            Flow::OpenDetail(id) => open_detail(&store, model, &id),
            Flow::Mutate(mutation) => {
                apply(&mut store, model, mutation);
                model.screen = Screen::List;
                reload(&store, &config, model)?;
            }
        }
        Ok(true)
    })?;
    Ok("Closed memory review.\n".to_string())
}

/// Re-lists records for the current filter and feeds them to the model.
fn reload(store: &Store, config: &CliConfig, model: &mut MemoryModel) -> CliResult<()> {
    let mut filter = MemoryListFilter::new(config.project.clone());
    if let Some(status) = model.filter.status() {
        filter = filter.with_status(status);
    }
    let records = store.list(filter).map_err(|error| error.to_string())?;
    model.set_rows(records.iter().map(row_from_record).collect());
    Ok(())
}

/// Loads a record's audit trail and opens the detail screen.
fn open_detail(store: &Store, model: &mut MemoryModel, id: &str) {
    let Some(row) = model.picker.selected_item().cloned() else {
        return;
    };
    let audit = match store.audit_log(id) {
        Ok(events) => events
            .iter()
            .map(|event| AuditRow {
                sequence: event.sequence(),
                action: audit_action_label(event.action()).to_string(),
                actor: event.actor().to_string(),
                reason: event.reason().map(str::to_string),
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    model.show_detail(row, audit);
}

/// Applies a review effect, recording success or the error in the status line.
fn apply(store: &mut Store, model: &mut MemoryModel, mutation: Mutation) {
    let actor = model.actor.clone();
    let outcome = match &mutation {
        Mutation::Approve { id } => store.review(id, ReviewDecision::approve(actor)),
        Mutation::Reject { id, reason } => store.review(id, ReviewDecision::reject(actor, reason)),
        Mutation::Archive { id, reason } => {
            store.review(id, ReviewDecision::archive(actor, reason))
        }
        Mutation::Edit { id, content } => {
            store.edit(id, MemoryEdit::replace_content(actor, content))
        }
    };
    match outcome {
        Ok(record) => model.set_status(format!(
            "{} → {}",
            record.entry().id(),
            model::status_label(record.entry().status())
        )),
        Err(error) => model.set_status(error.to_string()),
    }
}

fn row_from_record(record: &MemoryRecord) -> MemoryRow {
    let entry = record.entry();
    MemoryRow {
        id: entry.id().to_string(),
        status: entry.status(),
        kind: entry.kind(),
        content: entry.content().to_string(),
        tags: entry.tags().to_vec(),
    }
}

fn audit_action_label(action: codel00p_memory::MemoryAuditAction) -> &'static str {
    use codel00p_memory::MemoryAuditAction::*;
    match action {
        CandidateCreated => "candidate_created",
        Approved => "approved",
        Rejected => "rejected",
        Archived => "archived",
        Edited => "edited",
        Merged => "merged",
    }
}
