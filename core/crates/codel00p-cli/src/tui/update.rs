//! The pure update loop: `update(&mut App, Msg) -> Vec<Effect>`. No terminal, no
//! network, no async — every state change in the TUI flows through here, which is
//! what makes the behavior unit-testable without a real terminal.

use codel00p_harness::{HarnessEvent, PermissionDecision, PermissionMode, SessionState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;
use super::msg::{CloudFetch, Effect, LocalQuery, Msg};
use super::overlay::{EntityBrowser, EntityTab, ModelPicker, Overlay, SessionSwitcher};
use super::picker::PickerOutcome;

pub(crate) fn update(app: &mut App, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Key(key) => handle_key(app, key),
        Msg::Resize | Msg::Tick => {
            app.tick = app.tick.wrapping_add(1);
            Vec::new()
        }
        Msg::Token(token) => {
            app.conversation.append_token(&token);
            Vec::new()
        }
        Msg::Event(event) => {
            apply_event(app, &event);
            Vec::new()
        }
        Msg::Notice(text) => {
            app.conversation.push_notice(text);
            Vec::new()
        }
        Msg::TurnFinished(result) => handle_turn_finished(app, result),
        Msg::PermissionAsked(request, reply) => {
            app.overlay = Overlay::Permission(request);
            app.pending_permission = Some(reply);
            Vec::new()
        }
        Msg::CloudViewer(result) => {
            match result {
                Ok(viewer) => {
                    app.cloud.viewer = Some(viewer);
                    app.cloud.error = None;
                }
                Err(error) => app.cloud.error = Some(error),
            }
            Vec::new()
        }
        Msg::CloudProjects(result) => {
            with_entities(app, |browser| match result {
                Ok(projects) => {
                    browser.projects.set_items(projects);
                    browser.status = None;
                }
                Err(error) => browser.status = Some(error),
            });
            Vec::new()
        }
        Msg::CloudAgents(result) => {
            with_entities(app, |browser| match result {
                Ok(agents) => {
                    browser.agents.set_items(agents);
                    browser.status = None;
                }
                Err(error) => browser.status = Some(error),
            });
            Vec::new()
        }
        Msg::CloudMcp(result) => {
            with_entities(app, |browser| match result {
                Ok(servers) => browser.mcp.set_items(servers),
                Err(error) => browser.status = Some(error),
            });
            Vec::new()
        }
        Msg::CloudMemory(result) => {
            with_entities(app, |browser| match result {
                Ok(memory) => browser.memory.set_items(memory),
                Err(error) => browser.status = Some(error),
            });
            Vec::new()
        }
        Msg::CloudUsers(result) => {
            with_entities(app, |browser| match result {
                Ok(users) => browser.users.set_items(users),
                Err(error) => browser.status = Some(error),
            });
            Vec::new()
        }
        Msg::Models(result) => {
            // Drop the update if the user closed the picker before the fetch returned.
            if let Overlay::Model(picker) = &mut app.overlay {
                match result {
                    Ok(models) if !models.is_empty() => picker.set_choices(models, None),
                    // On error or an empty catalog, keep the static rows already shown
                    // and note why, so any model id stays reachable via free-text.
                    Ok(_) => picker.status = Some("No live models — showing catalog.".to_string()),
                    Err(error) => {
                        picker.status =
                            Some(format!("Catalog unavailable ({error}) — showing catalog."));
                    }
                }
            }
            Vec::new()
        }
        Msg::SessionList(result) => {
            if let Overlay::Sessions(switcher) = &mut app.overlay {
                match result {
                    Ok(sessions) if sessions.is_empty() => {
                        switcher.set_sessions(
                            Vec::new(),
                            Some("No saved conversations yet.".to_string()),
                        );
                    }
                    Ok(sessions) => switcher.set_sessions(sessions, None),
                    Err(error) => switcher.status = Some(error),
                }
            }
            Vec::new()
        }
        Msg::SessionResumed(result) => {
            match result {
                Ok((session_state, persisted)) => {
                    app.session_state = *session_state;
                    app.persisted_message_count = persisted;
                    app.conversation = super::conversation::Conversation::default();
                    app.turn = super::app::TurnStatus::default();
                    app.refresh_usage();
                    app.conversation.push_notice(format!(
                        "Resumed conversation {} ({} message(s)).",
                        app.session_label(),
                        persisted
                    ));
                }
                Err(error) => app.conversation.push_error(error),
            }
            Vec::new()
        }
    }
}

/// Runs `f` against the entity browser if one is open; otherwise drops the update
/// (the user closed the overlay before the fetch returned).
fn with_entities(app: &mut App, f: impl FnOnce(&mut EntityBrowser)) {
    if let Overlay::Entities(browser) = &mut app.overlay {
        f(browser);
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return vec![Effect::Quit];
    }
    if app.overlay.is_open() {
        handle_overlay_key(app, key)
    } else {
        handle_input_key(app, key)
    }
}

fn handle_overlay_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    // Take the overlay out so we can mutate both it and the rest of `app`, then put
    // it back unless the action closed it.
    let overlay = std::mem::replace(&mut app.overlay, Overlay::None);
    match overlay {
        Overlay::Help => Vec::new(), // any key (incl. Esc) closes
        Overlay::Permission(request) => {
            let allow = matches!(key.code, KeyCode::Char('y') | KeyCode::Char('a'));
            let deny = matches!(key.code, KeyCode::Char('n') | KeyCode::Esc);
            if !allow && !deny {
                app.overlay = Overlay::Permission(request);
                return Vec::new();
            }
            let decision = if allow {
                PermissionDecision::allow(request.id(), PermissionMode::Ask)
            } else {
                PermissionDecision::deny(
                    request.id(),
                    PermissionMode::Ask,
                    format!("{} denied in the approval modal", request.tool_name()),
                )
            };
            match app.pending_permission.take() {
                Some(reply) => vec![Effect::ResolvePermission(reply, decision)],
                None => Vec::new(),
            }
        }
        Overlay::Model(mut picker) => {
            // Enter with no highlighted row but a non-empty filter is a free-text
            // model id, mirroring the CLI's unchecked `/model <id>`.
            if key.code == KeyCode::Enter
                && picker.selected_item().is_none()
                && !picker.free_text().trim().is_empty()
            {
                let model = picker.free_text().trim().to_string();
                apply_model(app, &app.options.provider.clone(), &model);
                return Vec::new();
            }
            match picker.on_key(key) {
                PickerOutcome::Selected => {
                    if let Some(choice) = picker.selected_item() {
                        let (provider, model) = (choice.provider.clone(), choice.model.clone());
                        apply_model(app, &provider, &model);
                    }
                    Vec::new()
                }
                PickerOutcome::Cancelled => Vec::new(),
                PickerOutcome::Pending => {
                    app.overlay = Overlay::Model(picker);
                    Vec::new()
                }
            }
        }
        Overlay::Sessions(mut switcher) => match switcher.on_key(key) {
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
        },
        Overlay::Entities(browser) => handle_entities_key(app, browser, key),
        Overlay::None => Vec::new(),
    }
}

/// Sets the provider/model for later turns and notes the change. Centralized so the
/// catalog-row and free-text paths in the model picker stay consistent.
fn apply_model(app: &mut App, provider: &str, model: &str) {
    app.options.provider = provider.to_string();
    app.options.model = model.to_string();
    app.conversation.push_notice(format!(
        "Model set to {provider} · {model} (applies to the next turn)."
    ));
}

fn handle_entities_key(app: &mut App, mut browser: EntityBrowser, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::Tab => {
            browser.tab = browser.tab.next();
            app.overlay = Overlay::Entities(browser);
            return Vec::new();
        }
        KeyCode::BackTab => {
            browser.tab = browser.tab.prev();
            app.overlay = Overlay::Entities(browser);
            return Vec::new();
        }
        _ => {}
    }

    match browser.tab {
        EntityTab::Projects => match browser.projects.on_key(key) {
            PickerOutcome::Selected => {
                if let Some(project) = browser.projects.selected_item().cloned() {
                    let id = project.id().to_string();
                    browser.selected_project = Some(project);
                    browser.tab = EntityTab::Agents;
                    browser.status = Some("Loading project…".to_string());
                    app.overlay = Overlay::Entities(browser);
                    return vec![
                        Effect::Cloud(CloudFetch::Agents(id.clone())),
                        Effect::Cloud(CloudFetch::Mcp(id.clone())),
                        Effect::Cloud(CloudFetch::Memory(id)),
                    ];
                }
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
            PickerOutcome::Cancelled => Vec::new(),
            PickerOutcome::Pending => {
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
        },
        EntityTab::Agents => {
            match browser.agents.on_key(key) {
                PickerOutcome::Selected => {
                    if let Some(agent) = browser.agents.selected_item() {
                        app.options.provider = agent.provider().to_string();
                        app.options.model = agent.model().to_string();
                        let name = agent.name().to_string();
                        let provider = agent.provider().to_string();
                        let model = agent.model().to_string();
                        app.conversation.push_notice(format!(
                            "Agent “{name}” selected — provider {provider}, model {model} (applies next turn)."
                        ));
                    }
                    Vec::new() // close overlay
                }
                PickerOutcome::Cancelled => Vec::new(),
                PickerOutcome::Pending => {
                    app.overlay = Overlay::Entities(browser);
                    Vec::new()
                }
            }
        }
        EntityTab::Mcp => {
            if matches!(browser.mcp.on_key(key), PickerOutcome::Cancelled) {
                return Vec::new();
            }
            app.overlay = Overlay::Entities(browser);
            Vec::new()
        }
        EntityTab::Memory => {
            if matches!(browser.memory.on_key(key), PickerOutcome::Cancelled) {
                return Vec::new();
            }
            app.overlay = Overlay::Entities(browser);
            Vec::new()
        }
        EntityTab::Users => {
            if matches!(browser.users.on_key(key), PickerOutcome::Cancelled) {
                return Vec::new();
            }
            app.overlay = Overlay::Entities(browser);
            Vec::new()
        }
        EntityTab::Org => {
            if key.code == KeyCode::Esc {
                Vec::new()
            } else {
                app.overlay = Overlay::Entities(browser);
                Vec::new()
            }
        }
    }
}

fn handle_input_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::F(1) => {
            app.overlay = Overlay::Help;
            Vec::new()
        }
        KeyCode::F(2) => open_model_picker(app),
        KeyCode::F(3) => open_entities(app, EntityTab::Projects),
        KeyCode::F(4) => open_entities(app, EntityTab::Org),
        KeyCode::F(5) => open_sessions(app),
        KeyCode::Enter => {
            let line = app.input.trim().to_string();
            app.input.clear();
            if line.is_empty() {
                Vec::new()
            } else if let Some(command) = line.strip_prefix('/') {
                handle_slash(app, command)
            } else {
                submit_turn(app, line)
            }
        }
        KeyCode::Backspace => {
            app.input.pop();
            Vec::new()
        }
        KeyCode::Esc => {
            if app.input.is_empty() {
                vec![Effect::Quit]
            } else {
                app.input.clear();
                Vec::new()
            }
        }
        KeyCode::Char(c) => {
            app.input.push(c);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn submit_turn(app: &mut App, prompt: String) -> Vec<Effect> {
    if app.turn.running {
        app.conversation
            .push_notice("A turn is already running — wait for it to finish.");
        return Vec::new();
    }
    app.conversation.push_user(prompt.clone());
    app.turn = super::app::TurnStatus {
        running: true,
        ..Default::default()
    };
    vec![Effect::SubmitTurn(prompt)]
}

fn handle_slash(app: &mut App, command: &str) -> Vec<Effect> {
    let name = command.split_whitespace().next().unwrap_or("");
    match name {
        "model" => open_model_picker(app),
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
            app.conversation = super::conversation::Conversation::default();
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
fn open_model_picker(app: &mut App) -> Vec<Effect> {
    let choices = app.model_choices();
    let provider = app.options.provider.clone();
    app.overlay = Overlay::Model(ModelPicker::new(
        choices,
        Some("Loading models…".to_string()),
    ));
    vec![Effect::FetchModels(provider)]
}

fn open_sessions(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::Sessions(SessionSwitcher::new());
    vec![Effect::ListSessions]
}

fn open_entities(app: &mut App, tab: EntityTab) -> Vec<Effect> {
    if !app.cloud.configured {
        app.conversation.push_notice(
            "Cloud not configured — run `codel00p auth login` to browse org entities.",
        );
        return Vec::new();
    }
    app.overlay = Overlay::Entities(EntityBrowser::new(tab));
    vec![
        Effect::Cloud(CloudFetch::Viewer),
        Effect::Cloud(CloudFetch::Projects),
        Effect::Cloud(CloudFetch::Users),
    ]
}

fn apply_event(app: &mut App, event: &HarnessEvent) {
    use codel00p_protocol::AgentEvent::*;
    match event {
        ToolCallRequested { tool_name, .. } => {
            app.conversation.tool_requested(tool_name);
            app.turn.current_tool = Some(tool_name.clone());
        }
        ToolProgress {
            tool_name, message, ..
        } => app.conversation.tool_progress(tool_name, message.clone()),
        ToolCallCompleted { tool_name, .. } => {
            app.conversation.tool_completed(tool_name);
            app.turn.current_tool = None;
        }
        ToolCallFailed {
            tool_name, message, ..
        } => {
            app.conversation.tool_failed(tool_name, message);
            app.turn.current_tool = None;
        }
        PermissionDenied {
            tool_name, message, ..
        } => app
            .conversation
            .push_notice(format!("Permission denied for {tool_name}: {message}")),
        ContextCompacted {
            before_message_count,
            after_message_count,
            ..
        } => app.conversation.push_notice(format!(
            "Context compacted: {before_message_count} → {after_message_count} messages."
        )),
        InferenceCompleted { finish_reason, .. } => {
            app.turn.finish_reason = finish_reason.clone();
        }
        TurnCompleted { iterations, .. } => {
            app.turn.iterations = *iterations;
        }
        _ => {}
    }
}

fn handle_turn_finished(
    app: &mut App,
    result: Result<Box<codel00p_harness::TurnOutcome>, String>,
) -> Vec<Effect> {
    app.turn.running = false;
    app.turn.current_tool = None;
    match result {
        Ok(outcome) => {
            if let Some(message) = &outcome.assistant_message {
                app.conversation.finalize_assistant(message);
            }
            let start = app.persisted_message_count;
            app.session_state = outcome.session_state.clone();
            app.persisted_message_count = outcome.session_state.messages().len();
            app.refresh_usage();
            vec![Effect::Persist(outcome, start)]
        }
        Err(message) => {
            app.conversation.push_error(message);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::conversation::Block as ChatBlock;
    use crate::tui::test_support::test_app;
    use codel00p_harness::{SessionId, SessionState, TurnId, TurnOutcome};
    use codel00p_protocol::{OrgMember, OrgRole, PermissionRequest, PermissionScope};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_str(app: &mut App, text: &str) {
        for c in text.chars() {
            update(app, key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn typing_then_enter_submits_a_turn() {
        let mut app = test_app();
        type_str(&mut app, "hello");
        assert_eq!(app.input, "hello");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(effects.as_slice(), [Effect::SubmitTurn(p)] if p == "hello"));
        assert!(app.turn.running);
        assert!(app.input.is_empty());
        assert!(matches!(app.conversation.blocks.last(), Some(ChatBlock::User(t)) if t == "hello"));
    }

    #[test]
    fn submit_is_blocked_while_running() {
        let mut app = test_app();
        app.turn.running = true;
        type_str(&mut app, "again");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(effects.is_empty());
    }

    #[test]
    fn f2_opens_model_picker_and_selection_sets_model() {
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(2)));
        assert!(matches!(app.overlay, Overlay::Model(_)));
        // Move to the next catalog entry and select it.
        update(&mut app, key(KeyCode::Down));
        update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::None));
        // The model changed away from the initial "current" row.
        assert_ne!(app.options.model, "claude-opus-4-8");
    }

    #[test]
    fn streamed_tokens_append_to_conversation() {
        let mut app = test_app();
        update(&mut app, Msg::Token("par".into()));
        update(&mut app, Msg::Token("tial".into()));
        assert!(
            matches!(app.conversation.blocks.last(), Some(ChatBlock::Assistant(t)) if t == "partial")
        );
    }

    #[test]
    fn permission_request_opens_modal_and_allow_resolves() {
        let mut app = test_app();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let request = PermissionRequest::new(
            "perm-1",
            SessionId::new(),
            TurnId::new(),
            "run_command",
            serde_json::json!({}),
            PermissionScope::Shell,
        );
        update(&mut app, Msg::PermissionAsked(request, reply_tx));
        assert!(matches!(app.overlay, Overlay::Permission(_)));
        assert!(app.pending_permission.is_some());

        let effects = update(&mut app, key(KeyCode::Char('y')));
        assert!(matches!(app.overlay, Overlay::None));
        match effects.as_slice() {
            [Effect::ResolvePermission(_, decision)] => assert!(decision.allows_execution()),
            other => panic!("expected ResolvePermission, got {} effects", other.len()),
        }
        // Draining the effect would deliver the decision; the receiver is still live.
        drop(reply_rx);
    }

    #[test]
    fn turn_finished_persists_and_clears_running() {
        let mut app = test_app();
        app.turn.running = true;
        let mut session_state = SessionState::new(SessionId::from_static("session-x"));
        session_state.push_assistant("answer");
        let outcome = TurnOutcome {
            assistant_message: Some("answer".to_string()),
            tool_calls: Vec::new(),
            events: Vec::new(),
            session_state,
            cancelled: false,
        };
        let effects = update(&mut app, Msg::TurnFinished(Ok(Box::new(outcome))));
        assert!(!app.turn.running);
        assert_eq!(app.persisted_message_count, 1);
        assert!(matches!(effects.as_slice(), [Effect::Persist(_, 0)]));
    }

    #[test]
    fn slash_reset_starts_new_conversation() {
        let mut app = test_app();
        app.conversation.push_user("old");
        app.persisted_message_count = 4;
        type_str(&mut app, "/reset");
        update(&mut app, key(KeyCode::Enter));
        assert_eq!(app.persisted_message_count, 0);
        // Only the reset notice remains.
        assert!(matches!(
            app.conversation.blocks.as_slice(),
            [ChatBlock::Notice(_)]
        ));
    }

    #[test]
    fn esc_on_empty_input_quits() {
        let mut app = test_app();
        let effects = update(&mut app, key(KeyCode::Esc));
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
    }

    #[test]
    fn ctrl_c_quits_even_with_input() {
        let mut app = test_app();
        type_str(&mut app, "draft message");
        let ctrl_c = Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let effects = update(&mut app, ctrl_c);
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
    }

    #[test]
    fn esc_with_input_clears_instead_of_quitting() {
        let mut app = test_app();
        type_str(&mut app, "oops");
        let effects = update(&mut app, key(KeyCode::Esc));
        assert!(effects.is_empty());
        assert!(app.input.is_empty());
    }

    #[test]
    fn entities_without_cloud_shows_notice() {
        let mut app = test_app(); // cloud_configured = false
        let effects = update(&mut app, key(KeyCode::F(3)));
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Overlay::None));
        assert!(matches!(
            app.conversation.blocks.last(),
            Some(ChatBlock::Notice(_))
        ));
    }

    #[test]
    fn opening_entities_fetches_users_too() {
        let mut app = test_app();
        app.cloud.configured = true;
        let effects = update(&mut app, key(KeyCode::F(3)));

        assert!(matches!(app.overlay, Overlay::Entities(_)));
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Cloud(CloudFetch::Viewer),
                Effect::Cloud(CloudFetch::Projects),
                Effect::Cloud(CloudFetch::Users)
            ]
        ));
    }

    #[test]
    fn cloud_users_populates_entity_browser() {
        let mut app = test_app();
        app.overlay = Overlay::Entities(EntityBrowser::new(EntityTab::Users));

        update(
            &mut app,
            Msg::CloudUsers(Ok(vec![
                OrgMember::new("user_1", OrgRole::Admin)
                    .with_email("ada@example.com")
                    .with_name("Ada Lovelace"),
            ])),
        );

        match &app.overlay {
            Overlay::Entities(browser) => {
                let member = browser.users.selected_item().expect("selected member");
                assert_eq!(member.user_id(), "user_1");
                assert_eq!(member.name(), Some("Ada Lovelace"));
            }
            _ => panic!("expected entity browser"),
        }
    }

    #[test]
    fn f2_opens_model_picker_and_fetches_live_models() {
        let mut app = test_app();
        let effects = update(&mut app, key(KeyCode::F(2)));
        assert!(matches!(app.overlay, Overlay::Model(_)));
        // The static catalog is shown immediately and a live fetch is requested.
        assert!(matches!(
            effects.as_slice(),
            [Effect::FetchModels(provider)] if provider == "anthropic"
        ));
    }

    #[test]
    fn models_message_replaces_picker_choices() {
        use crate::tui::overlay::ModelChoice;
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(2)));
        update(
            &mut app,
            Msg::Models(Ok(vec![ModelChoice {
                provider: "anthropic".to_string(),
                model: "live-model-1".to_string(),
                note: Some("from catalog".to_string()),
            }])),
        );
        match &app.overlay {
            Overlay::Model(picker) => {
                assert!(picker.status.is_none());
                let choice = picker.selected_item().expect("a model row");
                assert_eq!(choice.model, "live-model-1");
            }
            _ => panic!("expected model picker"),
        }
    }

    #[test]
    fn empty_models_result_falls_back_to_static_catalog() {
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(2)));
        update(&mut app, Msg::Models(Ok(Vec::new())));
        match &app.overlay {
            Overlay::Model(picker) => {
                // Catalog rows are retained and a fallback note is shown.
                assert!(picker.selected_item().is_some());
                assert!(picker.status.as_deref().unwrap_or("").contains("catalog"));
            }
            _ => panic!("expected model picker"),
        }
    }

    #[test]
    fn model_picker_free_text_sets_any_model_id() {
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(2)));
        // Type an id no catalog row matches, then Enter.
        type_str(&mut app, "zzz-custom-model");
        update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.options.model, "zzz-custom-model");
    }

    #[test]
    fn switch_opens_session_switcher_and_lists() {
        let mut app = test_app();
        type_str(&mut app, "/switch");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::Sessions(_)));
        assert!(matches!(effects.as_slice(), [Effect::ListSessions]));
    }

    #[test]
    fn session_list_populates_switcher_and_selecting_resumes() {
        use crate::tui::overlay::SessionSummary;
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(5)));
        update(
            &mut app,
            Msg::SessionList(Ok(vec![SessionSummary {
                session_id: "chat-42".to_string(),
                source: "cli".to_string(),
                message_count: 3,
            }])),
        );
        // Selecting the highlighted row fires a resume for that session id.
        let effects = update(&mut app, key(KeyCode::Enter));
        match effects.as_slice() {
            [Effect::ResumeSession(id)] => assert_eq!(id.as_str(), "chat-42"),
            other => panic!("expected ResumeSession, got {} effects", other.len()),
        }
    }

    #[test]
    fn session_resumed_resets_conversation_and_usage() {
        let mut app = test_app();
        app.conversation.push_user("stale message");
        app.persisted_message_count = 9;
        let mut resumed = SessionState::new(SessionId::from_static("chat-42"));
        resumed.push_assistant("prior answer");
        update(&mut app, Msg::SessionResumed(Ok((Box::new(resumed), 1))));
        assert_eq!(app.session_label(), "chat-42");
        assert_eq!(app.persisted_message_count, 1);
        assert_eq!(app.usage.messages, 1);
        assert!(app.usage.estimated_tokens > 0);
        // Conversation was reset to just the resume notice.
        assert!(matches!(
            app.conversation.blocks.as_slice(),
            [ChatBlock::Notice(_)]
        ));
    }
}
