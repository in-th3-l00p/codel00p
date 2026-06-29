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

/// Handles keys in the agent overlay. In list mode the arrows move, Enter opens
/// the highlighted row (New → create form, an agent → live switch), `d` deletes a
/// non-active, non-default agent (with confirm), and Esc closes. The list is
/// arrow-navigated so the letter key is free for the delete action.
pub(super) fn handle_agent_switcher_key(
    app: &mut App,
    mut switcher: AgentSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    if switcher.confirm_delete.is_some() {
        return handle_agent_delete_key(app, switcher, key);
    }

    // `e` opens the detail/edit overlay for the highlighted agent.
    if key.code == KeyCode::Char('e') {
        if let Some(agent) = switcher.selected_agent() {
            let name = agent.name.clone();
            let is_default = name == super::super::agents::DEFAULT_AGENT_LABEL;
            app.overlay = Overlay::AgentDetail(AgentDetail::loading(name.clone(), is_default));
            return vec![Effect::LoadAgentDetail { name, is_default }];
        }
        app.overlay = Overlay::AgentSwitcher(switcher);
        return Vec::new();
    }

    // `d` opens a delete confirmation for the highlighted agent (guarded against
    // the active and default agents inside `begin_delete`).
    if key.code == KeyCode::Char('d') && switcher.selected_agent().is_some() {
        switcher.begin_delete(super::super::agents::DEFAULT_AGENT_LABEL);
        if switcher.confirm_delete.is_none() {
            app.conversation
                .push_notice("Can't delete the active or default agent.");
        }
        app.overlay = Overlay::AgentSwitcher(switcher);
        return Vec::new();
    }

    // Guard: never switch mid-turn (the home re-point must happen between turns).
    if key.code == KeyCode::Enter
        && app.turn.running
        && matches!(switcher.selected_row(), Some(AgentRow::Agent(_)))
    {
        app.conversation
            .push_notice("Can't switch agents while a turn is running — wait for it to finish.");
        app.overlay = Overlay::AgentSwitcher(switcher);
        return Vec::new();
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
                Some(AgentRow::New) => open_agent_creator(app),
                Some(AgentRow::Agent(agent)) if agent.active => {
                    app.conversation
                        .push_notice(format!("Already using agent “{}”.", agent.name));
                    Vec::new()
                }
                Some(AgentRow::Agent(agent)) => vec![Effect::SwitchAgent(agent.name.clone())],
                None => Vec::new(),
            },
            PickerOutcome::Cancelled => Vec::new(),
            PickerOutcome::Pending => {
                app.overlay = Overlay::AgentSwitcher(switcher);
                Vec::new()
            }
        },
        _ => {
            app.overlay = Overlay::AgentSwitcher(switcher);
            Vec::new()
        }
    }
}

/// Handles keys in the agent delete-confirmation prompt: `y` deletes, `n`/Esc
/// cancels.
fn handle_agent_delete_key(
    app: &mut App,
    mut switcher: AgentSwitcher,
    key: KeyEvent,
) -> Vec<Effect> {
    let Some(confirm) = switcher.confirm_delete.as_ref() else {
        app.overlay = Overlay::AgentSwitcher(switcher);
        return Vec::new();
    };
    match key.code {
        KeyCode::Char('y') => {
            let name = confirm.name.clone();
            switcher.confirm_delete = None;
            app.overlay = Overlay::AgentSwitcher(switcher);
            vec![Effect::DeleteAgent(name)]
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            switcher.confirm_delete = None;
            app.overlay = Overlay::AgentSwitcher(switcher);
            Vec::new()
        }
        _ => {
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

/// Handles keys in the agent detail/edit overlay: ↑/↓ or Tab move between fields,
/// printable chars / Backspace edit the focused field, Enter saves (dispatching
/// the write + returning to the list), Esc returns to the list without saving.
/// Edits are ignored until the on-disk values have loaded.
pub(super) fn handle_agent_detail_key(
    app: &mut App,
    mut detail: AgentDetail,
    key: KeyEvent,
) -> Vec<Effect> {
    // Esc always returns to the agent list (reloading it).
    if key.code == KeyCode::Esc {
        app.overlay = Overlay::AgentSwitcher(AgentSwitcher::new());
        return vec![Effect::ListAgents];
    }
    // Ignore edits until the values have loaded.
    if !detail.loaded {
        app.overlay = Overlay::AgentDetail(detail);
        return Vec::new();
    }
    match key.code {
        KeyCode::Up => {
            detail.move_field(false);
            app.overlay = Overlay::AgentDetail(detail);
            Vec::new()
        }
        KeyCode::Down | KeyCode::Tab => {
            detail.move_field(true);
            app.overlay = Overlay::AgentDetail(detail);
            Vec::new()
        }
        KeyCode::Enter => {
            let effect = Effect::SaveAgentDetail {
                name: detail.name.clone(),
                is_default: detail.is_default,
                description: detail.description.clone(),
                provider: detail.provider.clone(),
                model: detail.model.clone(),
                dispatch: detail.dispatch.clone(),
                persona: detail.persona.clone(),
            };
            // Return to the agent list (refreshed) after saving.
            app.overlay = Overlay::AgentSwitcher(AgentSwitcher::new());
            vec![effect, Effect::ListAgents]
        }
        KeyCode::Backspace => {
            detail.active_buffer_mut().pop();
            app.overlay = Overlay::AgentDetail(detail);
            Vec::new()
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            detail.active_buffer_mut().push(c);
            app.overlay = Overlay::AgentDetail(detail);
            Vec::new()
        }
        _ => {
            app.overlay = Overlay::AgentDetail(detail);
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
