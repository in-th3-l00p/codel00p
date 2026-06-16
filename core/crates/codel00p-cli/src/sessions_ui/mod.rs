//! The full-screen `codel00p sessions` browser dialog.
//!
//! Bare `codel00p sessions` (and `codel00p session`) opens this on a terminal;
//! the scriptable `list`/`show` subcommands remain for non-TTY use. The terminal
//! lifecycle and loop are shared via [`crate::dialog`]; this module loads the
//! session list and, on selection, a transcript to show read-only.

mod model;
#[cfg(test)]
mod tests;
mod view;

use codel00p_session::{PersistedSessionRecord, SessionRecord, SessionStore};

use crate::config::{CliConfig, CliResult, open_session_store, parse_session_id};
use crate::settings::AgentSettings;
use model::{Flow, SessionRow, SessionsModel};

type Store = codel00p_session::StorageBackedSessionStore<codel00p_storage::SqliteStorage>;

/// Runs the sessions browser. Lifecycle and loop come from [`crate::dialog`].
///
/// Selecting *resume* (`r`) closes the browser and continues that session in the
/// chat TUI, reusing the same launch path as `agent chat --session-id <id>`;
/// `agent_defaults` supplies the provider/model the chat needs.
pub(crate) fn run(config: CliConfig, agent_defaults: &AgentSettings) -> CliResult<String> {
    let store = open_session_store(&config)?;
    let mut model = SessionsModel::new();
    model.set_rows(load_rows(&store)?);

    // The synchronous dialog loop restores the terminal on exit; the resume id is
    // captured here so chat is launched *after* the browser has fully torn down.
    let mut resume: Option<String> = None;
    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Stay => {}
            Flow::Quit => return Ok(false),
            Flow::OpenDetail(id) => open_detail(&store, model, &id),
            Flow::Resume(id) => {
                resume = Some(id);
                return Ok(false);
            }
        }
        Ok(true)
    })?;

    drop(store);
    match resume {
        Some(id) => crate::agent::resume_chat(config, agent_defaults, &id),
        None => Ok("Closed sessions.\n".to_string()),
    }
}

/// Lists sessions with message/event counts, most-recent-first (undated last).
fn load_rows(store: &Store) -> CliResult<Vec<SessionRow>> {
    let mut rows = Vec::new();
    for metadata in store.list_sessions().map_err(|error| error.to_string())? {
        let records = store
            .replay(metadata.session_id())
            .map_err(|error| error.to_string())?;
        let messages = records
            .iter()
            .filter(|record| matches!(record.record(), SessionRecord::Message(_)))
            .count();
        rows.push((
            SessionRow {
                id: metadata.session_id().as_str().to_string(),
                source: metadata.source().to_string(),
                messages,
                events: records.len() - messages,
            },
            metadata.created_at(),
        ));
    }
    rows.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.id.cmp(&right.0.id))
    });
    Ok(rows.into_iter().map(|(row, _)| row).collect())
}

/// Loads the selected session's transcript and opens the detail screen.
fn open_detail(store: &Store, model: &mut SessionsModel, id: &str) {
    let Some(row) = model.picker.selected_item().cloned() else {
        return;
    };
    let Ok(session_id) = parse_session_id(id) else {
        return;
    };
    let transcript = match store.replay(&session_id) {
        Ok(records) => records.iter().map(transcript_line).collect(),
        Err(_) => Vec::new(),
    };
    model.show_detail(row, transcript);
}

fn transcript_line(record: &PersistedSessionRecord) -> String {
    match record.record() {
        SessionRecord::Message(message) => format!(
            "{}  {}  {}",
            record.sequence(),
            crate::session::session_role_label(message.role()),
            crate::session::session_message_summary(message)
        ),
        SessionRecord::Event(event) => format!(
            "{}  event  {}",
            record.sequence(),
            crate::session::agent_event_label(event)
        ),
    }
}
