//! Multi-agent persona overlays (#13): the agent switcher and the create-agent
//! form.
//!
//! Switching re-points the running TUI's home + memory, which must only happen
//! between turns, so the handlers here refuse mid-turn and emit
//! `Effect::SwitchAgent` / `Effect::ListAgents` for the event loop to carry out.

use super::*;

/// Opens the local-agent switcher (multi-agent personas, #13) and kicks off the
/// registry scan that fills it. Refuses mid-turn: switching re-points the home,
/// which must only happen between turns.
pub(super) fn open_agent_switcher(app: &mut App) -> Vec<Effect> {
    if app.turn.running {
        app.conversation
            .push_notice("Can't switch agents while a turn is running — wait for it to finish.");
        return Vec::new();
    }
    app.overlay = Overlay::AgentSwitcher(AgentSwitcher::new());
    vec![Effect::ListAgents]
}

/// Opens the create-agent form (multi-agent personas, #13).
pub(super) fn open_agent_creator(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::AgentCreate(AgentCreateForm::new());
    Vec::new()
}

/// Handles keys in the agent switcher: Enter live-switches to the highlighted
/// agent (re-pointing the running TUI's home + memory via `Effect::SwitchAgent`),
/// Esc closes. Selecting the already-active agent is a no-op notice.
pub(super) fn handle_agent_switcher_key(
    app: &mut App,
    mut switcher: AgentSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    // Guard: never switch mid-turn (the home re-point must happen between turns).
    if key.code == KeyCode::Enter && app.turn.running {
        app.conversation
            .push_notice("Can't switch agents while a turn is running — wait for it to finish.");
        app.overlay = Overlay::AgentSwitcher(switcher);
        return Vec::new();
    }
    match switcher.on_key(key) {
        PickerOutcome::Selected => match switcher.selected_item() {
            Some(agent) if agent.active => {
                app.conversation
                    .push_notice(format!("Already using agent “{}”.", agent.name));
                Vec::new()
            }
            Some(agent) => vec![Effect::SwitchAgent(agent.name.clone())],
            None => Vec::new(),
        },
        PickerOutcome::Cancelled => Vec::new(),
        PickerOutcome::Pending => {
            app.overlay = Overlay::AgentSwitcher(switcher);
            Vec::new()
        }
    }
}

/// Handles keys in the create-agent form: Tab moves between fields, printable
/// chars / Backspace edit the focused field, Enter validates the name and creates
/// the agent via the registry (then offers to switch to it), Esc cancels.
pub(super) fn handle_agent_create_key(
    app: &mut App,
    mut form: AgentCreateForm,
    key: KeyEvent,
) -> Vec<Effect> {
    match key.code {
        KeyCode::Esc => Vec::new(), // close
        KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
            form.focus_next();
            app.overlay = Overlay::AgentCreate(form);
            Vec::new()
        }
        KeyCode::Backspace => {
            form.backspace();
            app.overlay = Overlay::AgentCreate(form);
            Vec::new()
        }
        KeyCode::Char(c) => {
            form.push(c);
            app.overlay = Overlay::AgentCreate(form);
            Vec::new()
        }
        KeyCode::Enter => submit_agent_create(app, form),
        _ => {
            app.overlay = Overlay::AgentCreate(form);
            Vec::new()
        }
    }
}

/// Validates + creates the agent from the form, then live-switches to it. On a
/// validation or creation error, keeps the form open with the message shown.
fn submit_agent_create(app: &mut App, mut form: AgentCreateForm) -> Vec<Effect> {
    let name = form.name.trim().to_string();
    if let Err(error) = crate::agent::registry::validate_name(&name) {
        form.error = Some(error);
        app.overlay = Overlay::AgentCreate(form);
        return Vec::new();
    }
    let base = app.base_home.clone();
    let description = {
        let trimmed = form.description.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };
    let opts = crate::agent::registry::CreateOptions {
        description,
        ..Default::default()
    };
    match crate::agent::registry::create_agent(&base, &name, &opts) {
        Ok(_) => {
            app.conversation
                .push_notice(format!("Created agent “{name}”. Switching to it…"));
            // Offer-to-switch: a fresh agent is most useful live, so switch now.
            vec![Effect::SwitchAgent(name)]
        }
        Err(error) => {
            form.error = Some(error);
            app.overlay = Overlay::AgentCreate(form);
            Vec::new()
        }
    }
}
