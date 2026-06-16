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
    MemoryEdit, MemoryListFilter, MemoryMerge, MemoryRecord, MemoryRepository, ReviewDecision,
};
use codel00p_protocol::MemoryStatus;

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
            Flow::LoadMergeTargets(source) => load_merge_targets(&store, &config, model, &source)?,
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
                previous_content: event.previous_content().map(str::to_string),
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    model.show_detail(row, audit);
}

/// Loads the active records (approved/candidate) other than `source` and hands
/// them to the model as merge targets.
fn load_merge_targets(
    store: &Store,
    config: &CliConfig,
    model: &mut MemoryModel,
    source: &str,
) -> CliResult<()> {
    let mut targets = Vec::new();
    for status in [MemoryStatus::Candidate, MemoryStatus::Approved] {
        let filter = MemoryListFilter::new(config.project.clone()).with_status(status);
        let records = store.list(filter).map_err(|error| error.to_string())?;
        for record in &records {
            if record.entry().id() != source {
                targets.push(row_from_record(record));
            }
        }
    }
    model.show_merge_picker(targets);
    Ok(())
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
        Mutation::Merge { source, target } => store.merge(source, target, MemoryMerge::new(actor)),
        Mutation::Restore { id, sequence } => restore(store, &actor, id, *sequence),
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

/// Restores a record to the `previous_content` recorded at `sequence`, mirroring
/// the `memory restore` subcommand: find the audit entry, then `edit` back to it.
fn restore(
    store: &mut Store,
    actor: &str,
    id: &str,
    sequence: u64,
) -> Result<MemoryRecord, codel00p_memory::MemoryError> {
    let audit = store.audit_log(id)?;
    let previous_content = audit
        .iter()
        .find(|event| event.sequence() == sequence)
        .and_then(|event| event.previous_content())
        .ok_or_else(|| codel00p_memory::MemoryError::InvalidEdit {
            message: format!("audit sequence {sequence} has no previous content"),
        })?
        .to_string();
    let edit = MemoryEdit::replace_content(actor.to_string(), previous_content)
        .with_reason(format!("restore audit sequence {sequence}"));
    store.edit(id, edit)
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
