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

use std::collections::HashMap;

use codel00p_memory::{
    DEFAULT_CONSOLIDATION_THRESHOLD, MemoryEdit, MemoryListFilter, MemoryMerge, MemoryRecord,
    MemoryRepository, ReviewDecision, plan_consolidations,
};
use codel00p_protocol::MemoryStatus;

use crate::config::{CliConfig, CliResult, open_memory_store};
use model::{AuditRow, Flow, MemoryModel, MemoryRow, Mutation, NearDuplicate, Screen};

type Store = codel00p_memory::StorageBackedMemoryStore<codel00p_storage::SqliteStorage>;

/// Runs the review dialog. The terminal lifecycle and blocking loop come from
/// [`crate::dialog`]; the `on_key` closure performs the repository effects.
pub(crate) fn run(config: CliConfig) -> CliResult<String> {
    let mut store = open_memory_store(&config)?;
    let mut model = MemoryModel::new(crate::actor::infer_actor());
    // Surface near-duplicates only when the opt-in curator is enabled.
    let curator_on = std::env::current_dir()
        .ok()
        .map(|cwd| crate::skills::curator_enabled(&cwd))
        .unwrap_or(false);
    model.set_curator_enabled(curator_on);
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

/// Re-lists records for the current filter and feeds them to the model, tagging
/// any near-duplicate approved memories (when the curator is enabled).
fn reload(store: &Store, config: &CliConfig, model: &mut MemoryModel) -> CliResult<()> {
    let near_dups = if model.curator_enabled {
        consolidation_map(store, config)?
    } else {
        HashMap::new()
    };
    let mut filter = MemoryListFilter::new(config.project.clone());
    if let Some(status) = model.filter.status() {
        filter = filter.with_status(status);
    }
    let records = store.list(filter).map_err(|error| error.to_string())?;
    let rows = records
        .iter()
        .map(|record| {
            let mut row = row_from_record(record);
            row.near_duplicate_of = near_dups.get(row.id.as_str()).cloned();
            row
        })
        .collect();
    model.set_rows(rows);
    Ok(())
}

/// Maps each near-duplicate approved memory's id to its curator finding (survivor
/// + similarity), computed over the approved set via `plan_consolidations`.
fn consolidation_map(
    store: &Store,
    config: &CliConfig,
) -> CliResult<HashMap<String, NearDuplicate>> {
    let approved = store
        .list(MemoryListFilter::new(config.project.clone()).with_status(MemoryStatus::Approved))
        .map_err(|error| error.to_string())?;
    let mut map = HashMap::new();
    for consolidation in plan_consolidations(&approved, DEFAULT_CONSOLIDATION_THRESHOLD) {
        let survivor = consolidation.survivor().entry().id().to_string();
        for duplicate in consolidation.duplicates() {
            map.insert(
                duplicate.entry().id().to_string(),
                NearDuplicate {
                    survivor: survivor.clone(),
                    similarity: duplicate.similarity(),
                },
            );
        }
    }
    Ok(map)
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
        near_duplicate_of: None,
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
        Split => "split",
        EvidenceAdded => "evidence_added",
    }
}
