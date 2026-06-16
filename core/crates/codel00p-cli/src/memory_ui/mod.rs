//! The full-screen `codel00p memory` review dialog.
//!
//! Bare `codel00p memory` opens this on a terminal; the scriptable subcommands
//! (`list`, `approve`, ...) remain for non-TTY use. The pure model and rendering
//! live in [`model`] and [`view`]; this module owns the terminal lifecycle, the
//! blocking event loop, and the repository effects an update asks for.

mod model;
#[cfg(test)]
mod tests;
mod view;

use std::io::{self, Stdout};

use codel00p_memory::{
    MemoryEdit, MemoryListFilter, MemoryRecord, MemoryRepository, ReviewDecision,
};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::config::{CliConfig, CliResult, open_memory_store};
use model::{AuditRow, Flow, MemoryModel, MemoryRow, Mutation, Screen};

type Store = codel00p_memory::StorageBackedMemoryStore<codel00p_storage::SqliteStorage>;
type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Runs the review dialog, restoring the terminal on every exit path.
pub(crate) fn run(config: CliConfig) -> CliResult<String> {
    let mut store = open_memory_store(&config)?;
    let mut model = MemoryModel::new(crate::actor::infer_actor());
    reload(&store, &config, &mut model)?;

    let mut terminal =
        setup_terminal().map_err(|error| format!("terminal setup failed: {error}"))?;
    let result = event_loop(&mut model, &mut terminal, &mut store, &config);
    restore_terminal(&mut terminal);
    result.map(|_| "Closed memory review.\n".to_string())
}

fn event_loop(
    model: &mut MemoryModel,
    terminal: &mut Tui,
    store: &mut Store,
    config: &CliConfig,
) -> CliResult<()> {
    loop {
        terminal
            .draw(|frame| view::draw(frame, model))
            .map_err(|error| error.to_string())?;
        let Event::Key(key) = event::read().map_err(|error| error.to_string())? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match model.update(key) {
            Flow::Stay => {}
            Flow::Quit => return Ok(()),
            Flow::Reload => reload(store, config, model)?,
            Flow::OpenDetail(id) => open_detail(store, model, &id),
            Flow::Mutate(mutation) => {
                apply(store, model, mutation);
                model.screen = Screen::List;
                reload(store, config, model)?;
            }
        }
    }
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

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}
