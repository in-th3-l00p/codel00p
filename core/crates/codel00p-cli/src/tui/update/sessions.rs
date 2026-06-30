//! The conversations overlay: starting a new chat, resuming a prior one, and the
//! inline edit (name + description) and delete-confirm flows. `super::update`
//! dispatches the keys here; selections emit `Effect::ResumeSession` /
//! `Effect::EditSession` / `Effect::DeleteSession` for the event loop.

use super::*;

/// Handles keys in the conversations overlay. In list mode the arrows move, Enter
/// opens the highlighted row (New → fresh chat, a session → resume), `e`/F2 edit
/// the highlighted session, `d` deletes it, and Esc closes. The list is navigated
/// rather than type-filtered, so letter keys are free for actions.
pub(super) fn handle_sessions_key(
    app: &mut App,
    mut switcher: SessionSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    if switcher.edit.is_some() {
        return handle_session_edit_key(app, switcher, key);
    }
    if switcher.confirm_delete.is_some() {
        return handle_session_delete_key(app, switcher, key);
    }

    // `e`/F2 edit, `d` delete — but only when a real session (not New) is highlighted.
    match key.code {
        KeyCode::Char('e') | KeyCode::F(2) if switcher.selected_session().is_some() => {
            switcher.begin_edit();
            app.overlay = Overlay::Sessions(switcher);
            return Vec::new();
        }
        KeyCode::Char('d') if switcher.selected_session().is_some() => {
            switcher.begin_delete();
            app.overlay = Overlay::Sessions(switcher);
            return Vec::new();
        }
        _ => {}
    }

    // Only navigation / open / cancel keys reach the picker; letters stay free for
    // actions (the list is arrow-navigated, not type-filtered).
    match key.code {
        KeyCode::Up
        | KeyCode::Down
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Enter
        | KeyCode::Esc => match switcher.on_key(key) {
            PickerOutcome::Selected => match switcher.selected_row() {
                Some(ConversationRow::New) => handle_slash(app, "reset"),
                Some(ConversationRow::Session(session)) => {
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
        },
        _ => {
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
    }
}

/// Handles keys while editing a conversation's name + description: Tab switches
/// field, Enter saves (dispatching an edit), Esc cancels back to the list, and
/// printable characters / Backspace edit the focused field.
fn handle_session_edit_key(
    app: &mut App,
    mut switcher: SessionSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    let Some(edit) = switcher.edit.as_mut() else {
        app.overlay = Overlay::Sessions(switcher);
        return Vec::new();
    };
    match key.code {
        KeyCode::Esc => {
            switcher.edit = None;
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        KeyCode::Tab => {
            edit.toggle_field();
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        KeyCode::Enter => {
            let name = edit.name.split_whitespace().collect::<Vec<_>>().join(" ");
            let description = edit
                .description
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let id = edit.session_id.clone();
            switcher.edit = None;
            app.overlay = Overlay::Sessions(switcher);
            if name.is_empty() {
                app.conversation
                    .push_notice("A conversation needs a non-empty name.");
                return Vec::new();
            }
            match crate::config::parse_session_id(&id) {
                Ok(session_id) => vec![Effect::EditSession {
                    session_id,
                    title: name,
                    description,
                }],
                Err(error) => {
                    app.conversation
                        .push_error(format!("Cannot edit {id}: {error}"));
                    Vec::new()
                }
            }
        }
        KeyCode::Backspace => {
            edit.active_buffer_mut().pop();
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            edit.active_buffer_mut().push(c);
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
        _ => {
            app.overlay = Overlay::Sessions(switcher);
            Vec::new()
        }
    }
}

/// Handles keys in the delete-confirmation prompt: `y` deletes, `n`/Esc cancels.
fn handle_session_delete_key(
    app: &mut App,
    mut switcher: SessionSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    let Some(confirm) = switcher.confirm_delete.as_ref() else {
        app.overlay = Overlay::Sessions(switcher);
        return Vec::new();
    };
    match key.code {
        KeyCode::Char('y') => {
            let id = confirm.session_id.clone();
            switcher.confirm_delete = None;
            app.overlay = Overlay::Sessions(switcher);
            match crate::config::parse_session_id(&id) {
                Ok(session_id) => vec![Effect::DeleteSession(session_id)],
                Err(error) => {
                    app.conversation
                        .push_error(format!("Cannot delete {id}: {error}"));
                    Vec::new()
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            switcher.confirm_delete = None;
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
