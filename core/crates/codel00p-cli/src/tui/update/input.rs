//! The composer and transcript-input handlers: function keys that open overlays,
//! transcript scrolling/paging, line editing, and submitting a turn (or a slash
//! command). This is the default key path when no overlay is active; `super`
//! dispatches here and also drives scrolling from mouse-wheel messages.

use super::*;

pub(super) fn handle_input_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::F(1) => {
            app.overlay = Overlay::Help;
            Vec::new()
        }
        KeyCode::F(2) => open_model_picker(app),
        KeyCode::F(3) => open_entities(app, EntityTab::Projects),
        KeyCode::F(4) => open_entities(app, EntityTab::Org),
        KeyCode::F(5) => open_sessions(app),
        // Transcript scrolling.
        KeyCode::PageUp => {
            scroll_up(app, page_step(app));
            Vec::new()
        }
        KeyCode::PageDown => {
            scroll_down(app, page_step(app));
            Vec::new()
        }
        KeyCode::Enter => {
            // Alt/Shift+Enter inserts a newline; plain Enter submits.
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT)
            {
                app.composer.insert_newline();
                return Vec::new();
            }
            let line = app.composer.take();
            if line.is_empty() {
                Vec::new()
            } else if let Some(command) = line.strip_prefix('/') {
                handle_slash(app, command)
            } else {
                submit_turn(app, line)
            }
        }
        KeyCode::Backspace => {
            app.composer.backspace();
            Vec::new()
        }
        KeyCode::Delete => {
            app.composer.delete();
            Vec::new()
        }
        KeyCode::Left => {
            app.composer.left();
            Vec::new()
        }
        KeyCode::Right => {
            app.composer.right();
            Vec::new()
        }
        KeyCode::Home => {
            app.composer.home();
            Vec::new()
        }
        KeyCode::End => {
            app.composer.end();
            Vec::new()
        }
        KeyCode::Esc => {
            if app.composer.is_empty() {
                vec![Effect::Quit]
            } else {
                app.composer.clear();
                Vec::new()
            }
        }
        KeyCode::Char(c) => {
            app.composer.insert_char(c);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// One screenful of transcript rows for paging keys, from the last rendered
/// viewport height (minus one row of overlap). At least one row.
fn page_step(app: &App) -> u16 {
    app.viewport_rows.saturating_sub(1).max(1)
}

/// Scrolls the transcript up (toward older messages), leaving "follow" mode so the
/// view holds position instead of snapping back to the newest line.
pub(super) fn scroll_up(app: &mut App, rows: u16) {
    app.scroll.follow = false;
    app.scroll.offset_from_bottom = app.scroll.offset_from_bottom.saturating_add(rows);
}

/// Scrolls the transcript down (toward newer messages). Reaching the bottom
/// re-enables follow mode so new content keeps tailing.
pub(super) fn scroll_down(app: &mut App, rows: u16) {
    app.scroll.offset_from_bottom = app.scroll.offset_from_bottom.saturating_sub(rows);
    if app.scroll.offset_from_bottom == 0 {
        app.scroll.follow = true;
    }
}

fn submit_turn(app: &mut App, prompt: String) -> Vec<Effect> {
    if app.turn.running {
        app.conversation
            .push_notice("A turn is already running — wait for it to finish.");
        return Vec::new();
    }
    app.conversation.push_user(prompt.clone());
    app.scroll = super::super::app::ScrollState::default();
    app.turn = super::super::app::TurnStatus {
        running: true,
        started_tick: app.tick,
        ..Default::default()
    };
    vec![Effect::SubmitTurn(prompt)]
}
