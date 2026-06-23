//! The session switcher overlay: resuming a past session and the inline
//! rename flow. `super::update` dispatches the switcher keys here; selections
//! emit `Effect::ResumeSession` / `Effect::RenameSession` for the event loop.

use super::*;

/// Handles keys in the session switcher. In normal (list) mode, F2 enters an inline
/// rename for the highlighted row, Enter resumes it, Esc closes. In rename mode the
/// key edits the title buffer: Enter confirms (dispatching a rename), Esc cancels
/// back to the list, and printable characters / Backspace edit.
pub(super) fn handle_sessions_key(
    app: &mut App,
    mut switcher: SessionSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    if switcher.rename.is_some() {
        return handle_session_rename_key(app, switcher, key);
    }
    // F2 begins an inline rename of the highlighted session.
    if key.code == KeyCode::F(2) {
        switcher.begin_rename();
        app.overlay = Overlay::Sessions(switcher);
        return Vec::new();
    }
    match switcher.on_key(key) {
        PickerOutcome::Selected => match switcher.selected_item() {
            Some(session) => {
                let id = session.session_id.clone();
                match crate::config::parse_session_id(&id) {
                    Ok(session_id) => vec![Effect::ResumeSession(session_id)],
                    Err(error) => {
                        app.conversation
                            .push_error(format!("Cannot resume {id}: {error}"));
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        },
        PickerOutcome::Cancelled => Vec::new(),
        PickerOutcome::Pending => {
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
    }
}

/// Handles keys while inline-renaming a session: Enter confirms, Esc cancels, and
/// printable characters / Backspace edit the title buffer.
fn handle_session_rename_key(
    app: &mut App,
    mut switcher: SessionSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    let Some(rename) = switcher.rename.as_mut() else {
        app.overlay = Overlay::Sessions(switcher);
        return Vec::new();
    };
    match key.code {
        KeyCode::Esc => {
            // Cancel the rename but keep the switcher open on the list.
            switcher.rename = None;
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        KeyCode::Enter => {
            let title = rename
                .input
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let id = rename.session_id.clone();
            switcher.rename = None;
            app.overlay = Overlay::Sessions(switcher);
            if title.is_empty() {
                app.conversation
                    .push_notice("Rename needs a non-empty title.");
                return Vec::new();
            }
            match crate::config::parse_session_id(&id) {
                Ok(session_id) => vec![Effect::RenameSession(session_id, title)],
                Err(error) => {
                    app.conversation
                        .push_error(format!("Cannot rename {id}: {error}"));
                    Vec::new()
                }
            }
        }
        KeyCode::Backspace => {
            rename.input.pop();
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            rename.input.push(c);
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        _ => {
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
    }
}

pub(super) fn open_sessions(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::Sessions(SessionSwitcher::new());
    vec![Effect::ListSessions]
}
