//! The command palette and slash commands: the unified launcher that dispatches
//! to every action handler, the model picker, and `apply_model`.
//!
//! `run_command` (palette selection) and `handle_slash` (`/command` text) both
//! fan out to the open/handler functions in the sibling overlay modules, so this
//! is the one place that maps a user's intent to an action.

use super::*;

/// Opens the command palette overlay (the unified launcher).
pub(super) fn open_command_palette(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::Command(CommandPalette::new());
    Vec::new()
}

/// Dispatches a palette selection to the existing action handlers.
pub(super) fn run_command(app: &mut App, action: CommandAction) -> Vec<Effect> {
    match action {
        CommandAction::Model => open_model_picker(app),
        CommandAction::Sessions => open_sessions(app),
        CommandAction::NewConversation => handle_slash(app, "reset"),
        CommandAction::SwitchAgent => open_agent_switcher(app),
        CommandAction::CreateAgent => open_agent_creator(app),
        CommandAction::Browse => open_entities(app, EntityTab::Projects),
        CommandAction::Users => open_entities(app, EntityTab::Users),
        CommandAction::SwitchOrg => open_entities(app, EntityTab::Org),
        CommandAction::History => handle_slash(app, "history"),
        CommandAction::Tools => handle_slash(app, "tools"),
        CommandAction::Settings => open_settings(app),
        CommandAction::Help => {
            app.overlay = Overlay::Help;
            Vec::new()
        }
        CommandAction::Quit => vec![Effect::Quit],
    }
}

/// Sets the provider/model for later turns and notes the change. Centralized so the
/// catalog-row and free-text paths in the model picker stay consistent.
pub(super) fn apply_model(app: &mut App, provider: &str, model: &str) {
    app.options.provider = provider.to_string();
    app.options.model = model.to_string();
    app.conversation.push_notice(format!(
        "Model set to {provider} · {model} (applies to the next turn)."
    ));
}

pub(super) fn handle_slash(app: &mut App, command: &str) -> Vec<Effect> {
    let name = command.split_whitespace().next().unwrap_or("");
    match name {
        "model" => open_model_picker(app),
        // Local multi-agent personas (#13): switch / create the active agent.
        "agent" | "switch-agent" => open_agent_switcher(app),
        "new-agent" | "create-agent" => open_agent_creator(app),
        // `/agents` (plural) stays the cloud org agent browser.
        "agents" => open_entities(app, EntityTab::Agents),
        "org" => open_entities(app, EntityTab::Org),
        "entities" | "projects" => open_entities(app, EntityTab::Projects),
        "help" | "?" => {
            app.overlay = Overlay::Help;
            Vec::new()
        }
        "switch" => open_sessions(app),
        "sessions" => vec![Effect::Local(LocalQuery::Sessions)],
        "memory" => vec![Effect::Local(LocalQuery::Memory)],
        "history" => {
            let listing = crate::agent::chat_history_listing(&app.session_state);
            app.conversation.push_notice(listing.trim_end().to_string());
            Vec::new()
        }
        "tools" => {
            app.conversation.push_notice(format!(
                "Tool sets enabled: {}",
                if app.options.tool_sets.is_empty() {
                    "(none)".to_string()
                } else {
                    format!("{:?}", app.options.tool_sets)
                }
            ));
            Vec::new()
        }
        "reset" | "clear" => {
            let id = crate::config::parse_session_id(&crate::agent::fresh_chat_session_id())
                .unwrap_or_default();
            app.session_state = SessionState::new(id);
            app.persisted_message_count = 0;
            app.conversation = super::super::conversation::Conversation::default();
            app.scroll = super::super::app::ScrollState::default();
            app.conversation.push_notice(format!(
                "Started a new conversation ({}).",
                app.session_label()
            ));
            Vec::new()
        }
        "quit" | "exit" => vec![Effect::Quit],
        other => {
            app.conversation
                .push_notice(format!("Unknown command: /{other}. Press F1 for help."));
            Vec::new()
        }
    }
}

/// Opens the model picker pre-populated with the static catalog (so it is usable
/// instantly and offline) and kicks off a live `list_models` fetch that replaces the
/// rows on success — falling back to the catalog on error or an empty result.
pub(super) fn open_model_picker(app: &mut App) -> Vec<Effect> {
    let choices = app.model_choices();
    let provider = app.options.provider.clone();
    app.overlay = Overlay::Model(ModelPicker::new(
        choices,
        Some("Loading models…".to_string()),
    ));
    vec![Effect::FetchModels(provider)]
}
