//! The pure update loop: `update(&mut App, Msg) -> Vec<Effect>`. No terminal, no
//! network, no async — every state change in the TUI flows through here, which is
//! what makes the behavior unit-testable without a real terminal.

use codel00p_harness::{HarnessEvent, PermissionDecision, PermissionMode, SessionState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;
use super::msg::{CloudFetch, Effect, LocalQuery, Msg};
use super::overlay::{
    AdvancedKind, AdvancedPref, AdvancedSettingsOverlay, AgentCreateForm, AgentDetail, AgentRow,
    AgentSwitcher, ConversationRow, EntityBrowser, EntityTab, MainMenu, MenuSection, ModelPicker,
    Overlay, SessionSwitcher, SettingsOverlay, SettingsPref, SettingsRow, UpdatePrompt,
};
use super::picker::PickerOutcome;

mod agents;
mod commands;
mod entities;
mod events;
mod input;
mod sessions;
mod settings;
use agents::{
    handle_agent_create_key, handle_agent_detail_key, handle_agent_switcher_key,
    open_agent_creator, open_agent_switcher,
};
use events::{apply_event, handle_turn_finished};
use input::{handle_input_key, scroll_down, scroll_up};
use settings::{
    handle_advanced_settings_key, handle_settings_key, handle_update_prompt_key, open_settings,
};
// Glob re-exports: these three modules call one another's open/handler functions,
// so every sibling resolves them through the root namespace via `use super::*`.
use commands::*;
use entities::*;
use sessions::*;

pub(crate) fn update(app: &mut App, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Key(key) => handle_key(app, key),
        Msg::Scroll(delta) => {
            // Mouse wheel only scrolls the transcript when no overlay is focused.
            if !app.overlay.is_open() {
                if delta > 0 {
                    scroll_up(app, delta as u16);
                } else {
                    scroll_down(app, delta.unsigned_abs());
                }
            }
            Vec::new()
        }
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
        Msg::CloudOrgs(result) => {
            with_entities(app, |browser| match result {
                Ok(orgs) => {
                    browser.orgs.set_items(orgs);
                    browser.status = None;
                }
                Err(error) => browser.status = Some(error),
            });
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
        Msg::AgentList(result) => {
            if let Overlay::AgentSwitcher(switcher) = &mut app.overlay {
                match result {
                    Ok(agents) => switcher.set_agents(agents, None),
                    Err(error) => switcher.status = Some(error),
                }
            }
            Vec::new()
        }
        Msg::AgentDetailLoaded(result) => {
            if let Overlay::AgentDetail(detail) = &mut app.overlay {
                match result {
                    Ok(data) => detail.apply(data),
                    Err(error) => detail.status = Some(error),
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
                    app.scroll = super::app::ScrollState::default();
                    app.turn = super::app::TurnStatus::default();
                    app.refresh_usage();
                    app.conversation.push_notice(format!(
                        "Resumed conversation {} ({} message(s)).",
                        app.session_label(),
                        persisted
                    ));
                    app.conversation
                        .append_session_messages(app.session_state.messages());
                }
                Err(error) => app.conversation.push_error(error),
            }
            Vec::new()
        }
        Msg::UpdateAvailable(version) => {
            // Always record the version (drives the header chip), but only open the
            // prompt once per session — a later cache read / re-entry must not
            // reopen it after the user dismissed it.
            app.update_available = Some(version.clone());
            if !app.update_prompt_dismissed && !app.overlay.is_open() {
                app.overlay = Overlay::UpdatePrompt(UpdatePrompt {
                    current: crate::update::current_version().to_string(),
                    latest: version,
                });
            }
            Vec::new()
        }
        Msg::CheckUpdates => vec![Effect::CheckUpdates],
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return vec![Effect::Quit];
    }
    // Ctrl-P opens the top-level menu from anywhere — the four-section launcher,
    // so users do not have to remember the individual F-keys.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
        return open_menu(app);
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
        Overlay::Sessions(switcher) => handle_sessions_key(app, switcher, key),
        Overlay::Entities(browser) => handle_entities_key(app, browser, key),
        Overlay::Settings(settings) => handle_settings_key(app, settings, key),
        Overlay::AdvancedSettings(advanced) => handle_advanced_settings_key(app, advanced, key),
        Overlay::AgentSwitcher(switcher) => handle_agent_switcher_key(app, switcher, key),
        Overlay::AgentCreate(form) => handle_agent_create_key(app, form, key),
        Overlay::AgentDetail(detail) => handle_agent_detail_key(app, detail, key),
        Overlay::UpdatePrompt(prompt) => handle_update_prompt_key(app, prompt, key),
        Overlay::Menu(mut menu) => match menu.on_key(key) {
            PickerOutcome::Selected => match menu.selected_section() {
                Some(section) => open_section(app, section),
                None => Vec::new(),
            },
            PickerOutcome::Cancelled => Vec::new(),
            PickerOutcome::Pending => {
                app.overlay = Overlay::Menu(menu);
                Vec::new()
            }
        },
        Overlay::None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::conversation::Block as ChatBlock;
    use crate::tui::test_support::test_app;
    use codel00p_harness::{SessionId, SessionState, TurnId, TurnOutcome};
    use codel00p_protocol::{OrgMember, OrgRef, OrgRole, PermissionRequest, PermissionScope};
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
        assert_eq!(app.composer.text(), "hello");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(effects.as_slice(), [Effect::SubmitTurn(p)] if p == "hello"));
        assert!(app.turn.running);
        assert!(app.composer.is_empty());
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
        assert!(app.composer.is_empty());
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
                Effect::Cloud(CloudFetch::Orgs),
                Effect::Cloud(CloudFetch::Projects),
                Effect::Cloud(CloudFetch::Users)
            ]
        ));
    }

    #[test]
    fn cloud_orgs_populates_org_picker() {
        let mut app = test_app();
        app.overlay = Overlay::Entities(EntityBrowser::new(EntityTab::Org));

        update(
            &mut app,
            Msg::CloudOrgs(Ok(vec![OrgRef::new("org_acme", "Acme").with_slug("acme")])),
        );

        match &app.overlay {
            Overlay::Entities(browser) => {
                let org = browser.orgs.selected_item().expect("selected org");
                assert_eq!(org.id(), "org_acme");
                assert_eq!(org.name(), "Acme");
            }
            _ => panic!("expected entity browser"),
        }
    }

    #[test]
    fn selecting_org_requests_login_switch() {
        let mut app = test_app();
        app.cloud.configured = true;
        app.cloud.viewer = Some(
            codel00p_protocol::Viewer::new("user_1")
                .with_org(OrgRef::new("org_current", "Current"), OrgRole::Member),
        );
        let mut browser = EntityBrowser::new(EntityTab::Org);
        browser
            .orgs
            .set_items(vec![OrgRef::new("org_next", "Next")]);
        app.overlay = Overlay::Entities(browser);

        let effects = update(&mut app, key(KeyCode::Enter));

        assert!(matches!(
            effects.as_slice(),
            [Effect::SwitchOrg(org_id)] if org_id == "org_next"
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

    /// Builds a session switcher with one prior conversation and the highlight moved
    /// onto it (past the always-first "New conversation" row).
    fn switcher_with_one_session(app: &mut App, summary: crate::tui::overlay::SessionSummary) {
        update(app, key(KeyCode::F(5)));
        update(app, Msg::SessionList(Ok(vec![summary])));
        // Row 0 is "New conversation"; move down onto the prior conversation.
        update(app, key(KeyCode::Down));
    }

    fn sample_summary(id: &str, title: &str) -> crate::tui::overlay::SessionSummary {
        crate::tui::overlay::SessionSummary {
            session_id: id.to_string(),
            title: Some(title.to_string()),
            description: None,
            source: "cli".to_string(),
            message_count: 3,
        }
    }

    #[test]
    fn session_list_populates_switcher_and_selecting_resumes() {
        let mut app = test_app();
        switcher_with_one_session(&mut app, sample_summary("chat-42", "Fix switcher history"));
        // Selecting the highlighted session row fires a resume for that id.
        let effects = update(&mut app, key(KeyCode::Enter));
        match effects.as_slice() {
            [Effect::ResumeSession(id)] => assert_eq!(id.as_str(), "chat-42"),
            other => panic!("expected ResumeSession, got {} effects", other.len()),
        }
    }

    #[test]
    fn new_conversation_row_starts_a_fresh_chat() {
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(5)));
        update(
            &mut app,
            Msg::SessionList(Ok(vec![sample_summary("chat-1", "Old")])),
        );
        // Row 0 (New conversation) is highlighted by default; Enter resets the chat.
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Overlay::None));
        assert!(app.conversation.blocks.iter().any(|block| matches!(
            block,
            crate::tui::conversation::Block::Notice(text) if text.contains("new conversation")
        )));
    }

    #[test]
    fn e_enters_edit_and_enter_produces_edit_effect() {
        let mut app = test_app();
        switcher_with_one_session(&mut app, sample_summary("chat-7", "Old name"));
        // `e` enters edit mode seeded with the current name.
        update(&mut app, key(KeyCode::Char('e')));
        match &app.overlay {
            Overlay::Sessions(switcher) => {
                let edit = switcher.edit.as_ref().expect("edit mode");
                assert_eq!(edit.session_id, "chat-7");
                assert_eq!(edit.name, "Old name");
            }
            _ => panic!("expected session switcher"),
        }
        // Clear the name and type a fresh one.
        for _ in 0.."Old name".len() {
            update(&mut app, key(KeyCode::Backspace));
        }
        type_str(&mut app, "Fresh title");
        // Tab to the description field and type a description.
        update(&mut app, key(KeyCode::Tab));
        type_str(&mut app, "what it is about");
        let effects = update(&mut app, key(KeyCode::Enter));
        match effects.as_slice() {
            [
                Effect::EditSession {
                    session_id,
                    title,
                    description,
                },
            ] => {
                assert_eq!(session_id.as_str(), "chat-7");
                assert_eq!(title, "Fresh title");
                assert_eq!(description, "what it is about");
            }
            other => panic!("expected EditSession, got {} effects", other.len()),
        }
        assert!(matches!(app.overlay, Overlay::Sessions(_)));
    }

    #[test]
    fn d_confirms_then_deletes_the_session() {
        let mut app = test_app();
        switcher_with_one_session(&mut app, sample_summary("chat-7", "Doomed"));
        // `d` opens the confirm prompt; `y` confirms and emits the delete.
        update(&mut app, key(KeyCode::Char('d')));
        match &app.overlay {
            Overlay::Sessions(switcher) => {
                assert!(switcher.confirm_delete.is_some());
            }
            _ => panic!("expected session switcher"),
        }
        let effects = update(&mut app, key(KeyCode::Char('y')));
        match effects.as_slice() {
            [Effect::DeleteSession(id)] => assert_eq!(id.as_str(), "chat-7"),
            other => panic!("expected DeleteSession, got {} effects", other.len()),
        }
    }

    #[test]
    fn esc_cancels_edit_without_leaving_the_switcher() {
        let mut app = test_app();
        switcher_with_one_session(&mut app, sample_summary("chat-7", "Keep me"));
        update(&mut app, key(KeyCode::Char('e')));
        let effects = update(&mut app, key(KeyCode::Esc));
        assert!(effects.is_empty());
        match &app.overlay {
            Overlay::Sessions(switcher) => assert!(switcher.edit.is_none()),
            _ => panic!("expected session switcher to stay open"),
        }
    }

    #[test]
    fn session_resumed_loads_history_and_resets_usage() {
        let mut app = test_app();
        app.conversation.push_user("stale message");
        app.persisted_message_count = 9;
        let mut resumed = SessionState::new(SessionId::from_static("chat-42"));
        resumed.push_user(codel00p_harness::UserMessage::new("prior question"));
        resumed.push_assistant("prior answer");
        update(&mut app, Msg::SessionResumed(Ok((Box::new(resumed), 2))));
        assert_eq!(app.session_label(), "chat-42");
        assert_eq!(app.persisted_message_count, 2);
        assert_eq!(app.usage.messages, 2);
        assert!(app.usage.estimated_tokens > 0);
        assert_eq!(
            app.conversation.blocks,
            vec![
                ChatBlock::Notice("Resumed conversation chat-42 (2 message(s)).".to_string()),
                ChatBlock::User("prior question".to_string()),
                ChatBlock::Assistant("prior answer".to_string()),
            ]
        );
    }

    #[test]
    fn page_up_scrolls_and_leaves_follow_then_page_down_resumes() {
        let mut app = test_app();
        app.viewport_rows = 10;
        update(&mut app, key(KeyCode::PageUp));
        assert!(!app.scroll.follow);
        assert!(app.scroll.offset_from_bottom > 0);
        // Page back down past the top of the scrollback returns to following.
        update(&mut app, key(KeyCode::PageDown));
        update(&mut app, key(KeyCode::PageDown));
        assert_eq!(app.scroll.offset_from_bottom, 0);
        assert!(app.scroll.follow);
    }

    #[test]
    fn mouse_wheel_scrolls_the_transcript() {
        let mut app = test_app();
        app.viewport_rows = 10;
        update(&mut app, Msg::Scroll(3));
        assert!(!app.scroll.follow);
        assert_eq!(app.scroll.offset_from_bottom, 3);
        update(&mut app, Msg::Scroll(-3));
        assert_eq!(app.scroll.offset_from_bottom, 0);
        assert!(app.scroll.follow);
    }

    #[test]
    fn alt_enter_inserts_newline_without_submitting() {
        let mut app = test_app();
        type_str(&mut app, "line one");
        let alt_enter = Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
        let effects = update(&mut app, alt_enter);
        assert!(effects.is_empty());
        assert!(!app.turn.running);
        type_str(&mut app, "line two");
        assert_eq!(app.composer.text(), "line one\nline two");
    }

    #[test]
    fn submitting_a_turn_re_pins_to_the_latest() {
        let mut app = test_app();
        app.scroll.follow = false;
        app.scroll.offset_from_bottom = 5;
        type_str(&mut app, "hi");
        update(&mut app, key(KeyCode::Enter));
        assert!(app.scroll.follow);
        assert_eq!(app.scroll.offset_from_bottom, 0);
    }

    fn ctrl(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    #[test]
    fn ctrl_p_opens_the_menu() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        assert!(matches!(app.overlay, Overlay::Menu(_)));
    }

    #[test]
    fn selecting_a_section_opens_its_overlay() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        // The first section is "Agent"; Enter opens the agent switcher.
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::AgentSwitcher(_)));
        assert!(matches!(effects.as_slice(), [Effect::ListAgents]));
    }

    #[test]
    fn menu_filters_to_conversations_section() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        // Filter down to the Conversations section and open it.
        type_str(&mut app, "conv");
        update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::Sessions(_)));
    }

    #[test]
    fn settings_section_opens_settings_overlay() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        // Filter down to "Settings" and open it.
        type_str(&mut app, "settings");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Overlay::Settings(_)));
    }

    #[test]
    fn toggling_setting_flips_show_advanced_and_persists() {
        // The toggle persists to the user config; isolate CODEL00P_HOME so the
        // write lands in a tempdir rather than the real home (serialized with the
        // other env-touching tests via the shared lock).
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            assert!(!app.show_advanced);
            update(&mut app, ctrl(KeyCode::Char('p')));
            type_str(&mut app, "settings");
            update(&mut app, key(KeyCode::Enter));
            // Enter on the highlighted preference flips it on (and stays open).
            update(&mut app, key(KeyCode::Enter));
            assert!(app.show_advanced);
            assert!(matches!(app.overlay, Overlay::Settings(_)));
            // The change was written to the isolated config.
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.tui.show_advanced, Some(true));

            // Space toggles it back off.
            update(&mut app, key(KeyCode::Char(' ')));
            assert!(!app.show_advanced);
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.tui.show_advanced, Some(false));

            // Esc closes the overlay.
            update(&mut app, key(KeyCode::Esc));
            assert!(matches!(app.overlay, Overlay::None));
        });
    }

    #[test]
    fn settings_overlay_lists_and_toggles_check_updates() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            assert!(app.check_updates); // default on
            update(&mut app, ctrl(KeyCode::Char('p')));
            type_str(&mut app, "settings");
            update(&mut app, key(KeyCode::Enter));
            // Move to the "Check for updates" row (second preference) and toggle it.
            update(&mut app, key(KeyCode::Down));
            match &app.overlay {
                Overlay::Settings(settings) => {
                    assert_eq!(
                        settings.current(),
                        SettingsRow::Pref(SettingsPref::CheckUpdates)
                    );
                }
                _ => panic!("expected settings overlay"),
            }
            update(&mut app, key(KeyCode::Enter));
            assert!(!app.check_updates);
            // The change was written to the isolated config.
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.tui.check_updates, Some(false));
            // Space toggles it back on.
            update(&mut app, key(KeyCode::Char(' ')));
            assert!(app.check_updates);
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.tui.check_updates, Some(true));
        });
    }

    #[test]
    fn settings_advanced_row_opens_and_esc_returns() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        type_str(&mut app, "settings");
        update(&mut app, key(KeyCode::Enter));
        // Move to the "Advanced…" row (the last main row) and open it.
        update(&mut app, key(KeyCode::Up)); // wraps to last row
        match &app.overlay {
            Overlay::Settings(settings) => assert_eq!(settings.current(), SettingsRow::Advanced),
            _ => panic!("expected settings overlay"),
        }
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Overlay::AdvancedSettings(_)));
        // Esc returns to the main Settings overlay, not all the way closed.
        update(&mut app, key(KeyCode::Esc));
        assert!(matches!(app.overlay, Overlay::Settings(_)));
        // A second Esc closes Settings.
        update(&mut app, key(KeyCode::Esc));
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[test]
    fn advanced_edits_max_iterations_and_persists() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            assert_eq!(app.max_iterations, 25);
            app.overlay = Overlay::AdvancedSettings(AdvancedSettingsOverlay::new());
            // First row is Max iterations. Right increments by 1.
            update(&mut app, key(KeyCode::Right));
            assert_eq!(app.max_iterations, 26);
            // It persisted to the key the harness reads.
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.agent.max_iterations, Some(26));
            // Left decrements; `-` also works.
            update(&mut app, key(KeyCode::Left));
            update(&mut app, key(KeyCode::Char('-')));
            assert_eq!(app.max_iterations, 24);
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.agent.max_iterations, Some(24));
        });
    }

    #[test]
    fn advanced_max_iterations_floors_at_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            app.max_iterations = 1;
            app.overlay = Overlay::AdvancedSettings(AdvancedSettingsOverlay::new());
            // Decrement repeatedly: it cannot drop below the floor of 1.
            for _ in 0..5 {
                update(&mut app, key(KeyCode::Left));
            }
            assert_eq!(app.max_iterations, 1);
        });
    }

    #[test]
    fn advanced_toggles_bool_pref_and_persists() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            assert!(app.self_knowledge); // default on
            app.overlay = Overlay::AdvancedSettings(AdvancedSettingsOverlay::new());
            // Move down to the first boolean row (Self-knowledge, 4th row) and
            // toggle it off.
            for _ in 0..3 {
                update(&mut app, key(KeyCode::Down));
            }
            match &app.overlay {
                Overlay::AdvancedSettings(advanced) => {
                    assert_eq!(advanced.current(), AdvancedPref::SelfKnowledge);
                }
                _ => panic!("expected advanced overlay"),
            }
            update(&mut app, key(KeyCode::Enter));
            assert!(!app.self_knowledge);
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(resolved.merged.agent.behavior.self_knowledge, Some(false));
        });
    }

    #[test]
    fn update_available_opens_prompt_and_sets_version() {
        let mut app = test_app();
        assert!(app.update_available.is_none());
        update(&mut app, Msg::UpdateAvailable("9.9.9".to_string()));
        assert_eq!(app.update_available.as_deref(), Some("9.9.9"));
        match &app.overlay {
            Overlay::UpdatePrompt(prompt) => assert_eq!(prompt.latest, "9.9.9"),
            _ => panic!("expected the update prompt to open"),
        }
    }

    #[test]
    fn dismissing_update_prompt_closes_and_blocks_reopen() {
        let mut app = test_app();
        update(&mut app, Msg::UpdateAvailable("9.9.9".to_string()));
        assert!(matches!(app.overlay, Overlay::UpdatePrompt(_)));
        // Esc dismisses: the overlay closes, the dismissed flag is set, and the
        // version (header chip) is retained.
        let effects = update(&mut app, key(KeyCode::Esc));
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Overlay::None));
        assert!(app.update_prompt_dismissed);
        assert_eq!(app.update_available.as_deref(), Some("9.9.9"));
        // A second notification does not reopen the prompt after dismissal.
        update(&mut app, Msg::UpdateAvailable("9.9.9".to_string()));
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[test]
    fn update_now_sets_run_on_exit_flag_and_quits() {
        let mut app = test_app();
        update(&mut app, Msg::UpdateAvailable("9.9.9".to_string()));
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
        assert!(app.run_update_on_exit);
    }

    #[test]
    fn start_update_check_produces_check_effect() {
        let mut app = test_app();
        let effects = update(&mut app, Msg::CheckUpdates);
        assert!(matches!(effects.as_slice(), [Effect::CheckUpdates]));
    }

    #[test]
    fn inference_completed_captures_real_usage() {
        use codel00p_protocol::{AgentEvent, EventId, SessionId as PSessionId, TokenUsage, TurnId};
        let mut app = test_app();
        assert!(app.last_usage.is_none());
        let event = AgentEvent::InferenceCompleted {
            event_id: EventId::new(),
            session_id: PSessionId::new(),
            turn_id: TurnId::new(),
            finish_reason: Some("stop".to_string()),
            usage: Some(TokenUsage {
                input_tokens: 1000,
                output_tokens: 200,
                ..TokenUsage::default()
            }),
            cost: None,
        };
        update(&mut app, Msg::Event(event));
        let usage = app.last_usage.expect("usage captured");
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.total_tokens(), 1200);
    }

    #[test]
    fn inference_completed_captures_cost_when_present() {
        use codel00p_protocol::{
            AgentEvent, CostEstimate, EventId, SessionId as PSessionId, TurnId,
        };
        let mut app = test_app();
        assert!(app.last_cost.is_none());
        let event = AgentEvent::InferenceCompleted {
            event_id: EventId::new(),
            session_id: PSessionId::new(),
            turn_id: TurnId::new(),
            finish_reason: Some("stop".to_string()),
            usage: None,
            cost: Some(CostEstimate {
                currency: "USD".to_string(),
                total_nanos: 2_100_000,
            }),
        };
        update(&mut app, Msg::Event(event));
        let cost = app.last_cost.expect("cost captured");
        assert_eq!(cost.total_nanos, 2_100_000);
    }

    #[test]
    fn verification_completed_sets_status_for_pass_and_fail() {
        use codel00p_protocol::{AgentEvent, EventId, SessionId as PSessionId, TurnId};
        // A passing verification shows a clear ✓ verdict.
        let mut app = test_app();
        let pass = AgentEvent::VerificationCompleted {
            event_id: EventId::new(),
            session_id: PSessionId::new(),
            turn_id: TurnId::new(),
            check: "test".to_string(),
            command: "cargo test".to_string(),
            success: true,
            exit_code: Some(0),
            attempt: 1,
        };
        update(&mut app, Msg::Event(pass));
        let status = app.verification.clone().expect("verification status set");
        assert!(status.contains('✓'));
        assert!(status.contains("test"));

        // A failing verification flips to a ⚠ retrying verdict.
        let fail = AgentEvent::VerificationCompleted {
            event_id: EventId::new(),
            session_id: PSessionId::new(),
            turn_id: TurnId::new(),
            check: "test".to_string(),
            command: "cargo test".to_string(),
            success: false,
            exit_code: Some(101),
            attempt: 2,
        };
        update(&mut app, Msg::Event(fail));
        let status = app.verification.clone().expect("verification status set");
        assert!(status.contains('⚠'));
        assert!(status.contains("retrying"));
    }

    #[test]
    fn failure_budget_exceeded_shows_replanning_note() {
        use codel00p_protocol::{AgentEvent, EventId, SessionId as PSessionId, TurnId};
        let mut app = test_app();
        let event = AgentEvent::FailureBudgetExceeded {
            event_id: EventId::new(),
            session_id: PSessionId::new(),
            turn_id: TurnId::new(),
            operation: "run_command:cargo build".to_string(),
            attempts: 3,
            error_kind: "compile_error".to_string(),
        };
        update(&mut app, Msg::Event(event));
        let status = app.verification.clone().expect("verification status set");
        assert!(status.contains("replanning"));
    }

    #[test]
    fn settings_profile_row_cycles_and_persists() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            // The built-in presets are seeded; none active to start.
            assert!(app.active_profile.is_none());
            assert!(!app.profile_names.is_empty());
            update(&mut app, ctrl(KeyCode::Char('p')));
            type_str(&mut app, "settings");
            update(&mut app, key(KeyCode::Enter));
            // Move to the "Agent profile" row (third row) and cycle it forward.
            update(&mut app, key(KeyCode::Down));
            update(&mut app, key(KeyCode::Down));
            match &app.overlay {
                Overlay::Settings(settings) => {
                    assert_eq!(settings.current(), SettingsRow::Profile);
                }
                _ => panic!("expected settings overlay"),
            }
            update(&mut app, key(KeyCode::Enter));
            // Forward from no selection picks the first name.
            let first = app.profile_names[0].clone();
            assert_eq!(app.active_profile.as_deref(), Some(first.as_str()));
            // It persisted to `agent.profile` (the key the harness reads).
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(
                resolved.merged.agent.profile.as_deref(),
                Some(first.as_str())
            );
            // Right cycles to the next name; overlay stays open.
            update(&mut app, key(KeyCode::Right));
            assert_eq!(
                app.active_profile.as_deref(),
                Some(app.profile_names[1].as_str())
            );
            assert!(matches!(app.overlay, Overlay::Settings(_)));
            let resolved = crate::settings::load_layered(dir.path()).expect("reload");
            assert_eq!(
                resolved.merged.agent.profile.as_deref(),
                Some(app.profile_names[1].as_str())
            );
        });
    }

    // ---- Multi-agent personas (#13) phase 3: switch / create from the menu ----

    #[test]
    fn menu_lists_the_four_sections() {
        use crate::tui::overlay::MenuSection;
        let labels: Vec<_> = MenuSection::ORDER.iter().map(|s| s.label()).collect();
        assert_eq!(
            labels,
            ["Agent", "Conversations", "Organization", "Settings"]
        );
    }

    #[test]
    fn agent_section_opens_switcher_and_lists() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        type_str(&mut app, "agent");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::AgentSwitcher(_)));
        assert!(matches!(effects.as_slice(), [Effect::ListAgents]));
    }

    #[test]
    fn slash_agent_opens_switcher() {
        let mut app = test_app();
        type_str(&mut app, "/agent");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::AgentSwitcher(_)));
        assert!(matches!(effects.as_slice(), [Effect::ListAgents]));
    }

    #[test]
    fn agent_list_populates_switcher_and_selecting_emits_switch() {
        use crate::tui::overlay::AgentChoice;
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(1))); // any non-agent overlay; reset below
        app.overlay = Overlay::None;
        let _ = open_agent_switcher(&mut app);
        update(
            &mut app,
            Msg::AgentList(Ok(vec![
                AgentChoice {
                    name: "default".to_string(),
                    description: None,
                    active: true,
                },
                AgentChoice {
                    name: "scout".to_string(),
                    description: Some("recon".to_string()),
                    active: false,
                },
            ])),
        );
        // Rows are [New, default (active), scout]; move down twice onto scout,
        // then Enter switches to it.
        update(&mut app, key(KeyCode::Down));
        update(&mut app, key(KeyCode::Down));
        let effects = update(&mut app, key(KeyCode::Enter));
        match effects.as_slice() {
            [Effect::SwitchAgent(name)] => assert_eq!(name, "scout"),
            other => panic!("expected SwitchAgent, got {} effects", other.len()),
        }
    }

    #[test]
    fn new_agent_row_opens_the_create_form() {
        use crate::tui::overlay::AgentChoice;
        let mut app = test_app();
        let _ = open_agent_switcher(&mut app);
        update(
            &mut app,
            Msg::AgentList(Ok(vec![AgentChoice {
                name: "default".to_string(),
                description: None,
                active: true,
            }])),
        );
        // Row 0 (New agent) is highlighted by default; Enter opens the create form.
        update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::AgentCreate(_)));
    }

    #[test]
    fn selecting_the_active_agent_is_a_noop_notice() {
        use crate::tui::overlay::AgentChoice;
        let mut app = test_app();
        let _ = open_agent_switcher(&mut app);
        update(
            &mut app,
            Msg::AgentList(Ok(vec![AgentChoice {
                name: "default".to_string(),
                description: None,
                active: true,
            }])),
        );
        // Move past the New row onto the active default agent; Enter is a no-op.
        update(&mut app, key(KeyCode::Down));
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(effects.is_empty());
    }

    #[test]
    fn d_deletes_a_non_active_agent_after_confirm() {
        use crate::tui::overlay::AgentChoice;
        let mut app = test_app();
        let _ = open_agent_switcher(&mut app);
        update(
            &mut app,
            Msg::AgentList(Ok(vec![
                AgentChoice {
                    name: "default".to_string(),
                    description: None,
                    active: true,
                },
                AgentChoice {
                    name: "scout".to_string(),
                    description: None,
                    active: false,
                },
            ])),
        );
        // Move onto scout (row 2) and delete it.
        update(&mut app, key(KeyCode::Down));
        update(&mut app, key(KeyCode::Down));
        update(&mut app, key(KeyCode::Char('d')));
        match &app.overlay {
            Overlay::AgentSwitcher(switcher) => assert!(switcher.confirm_delete.is_some()),
            _ => panic!("expected the agent overlay with a pending confirm"),
        }
        let effects = update(&mut app, key(KeyCode::Char('y')));
        match effects.as_slice() {
            [Effect::DeleteAgent(name)] => assert_eq!(name, "scout"),
            other => panic!("expected DeleteAgent, got {} effects", other.len()),
        }
    }

    #[test]
    fn e_opens_agent_detail_and_loads_it() {
        use crate::tui::overlay::{AgentChoice, AgentDetailData};
        let mut app = test_app();
        let _ = open_agent_switcher(&mut app);
        update(
            &mut app,
            Msg::AgentList(Ok(vec![
                AgentChoice {
                    name: "default".to_string(),
                    description: None,
                    active: true,
                },
                AgentChoice {
                    name: "scout".to_string(),
                    description: Some("recon".to_string()),
                    active: false,
                },
            ])),
        );
        // Move onto scout and press `e` → detail overlay opens + a load effect fires.
        update(&mut app, key(KeyCode::Down));
        update(&mut app, key(KeyCode::Down));
        let effects = update(&mut app, key(KeyCode::Char('e')));
        match effects.as_slice() {
            [Effect::LoadAgentDetail { name, is_default }] => {
                assert_eq!(name, "scout");
                assert!(!is_default);
            }
            other => panic!("expected LoadAgentDetail, got {} effects", other.len()),
        }
        assert!(matches!(app.overlay, Overlay::AgentDetail(_)));
        // The loaded values populate the fields; editing then Enter saves them.
        update(
            &mut app,
            Msg::AgentDetailLoaded(Ok(AgentDetailData {
                description: "recon".to_string(),
                provider: "anthropic".to_string(),
                model: "claude-opus-4-8".to_string(),
                dispatch: String::new(),
                persona: "You scout.".to_string(),
                memory_note: "no memory recorded yet".to_string(),
            })),
        );
        match &app.overlay {
            Overlay::AgentDetail(detail) => {
                assert!(detail.loaded);
                assert_eq!(detail.model, "claude-opus-4-8");
            }
            _ => panic!("expected the detail overlay"),
        }
        let effects = update(&mut app, key(KeyCode::Enter));
        let saved = effects
            .iter()
            .any(|e| matches!(e, Effect::SaveAgentDetail { name, .. } if name == "scout"));
        assert!(saved, "Enter should emit a SaveAgentDetail for scout");
    }

    #[test]
    fn d_refuses_to_delete_the_active_agent() {
        use crate::tui::overlay::AgentChoice;
        let mut app = test_app();
        let _ = open_agent_switcher(&mut app);
        update(
            &mut app,
            Msg::AgentList(Ok(vec![AgentChoice {
                name: "default".to_string(),
                description: None,
                active: true,
            }])),
        );
        // Move onto the active default agent and try to delete — refused, no confirm.
        update(&mut app, key(KeyCode::Down));
        update(&mut app, key(KeyCode::Char('d')));
        match &app.overlay {
            Overlay::AgentSwitcher(switcher) => assert!(switcher.confirm_delete.is_none()),
            _ => panic!("expected the agent overlay to stay open"),
        }
    }

    #[test]
    fn cannot_open_switcher_mid_turn() {
        let mut app = test_app();
        app.turn.running = true;
        let effects = open_agent_switcher(&mut app);
        assert!(effects.is_empty());
        assert!(matches!(app.overlay, Overlay::None));
    }

    #[test]
    fn create_agent_form_rejects_invalid_name() {
        let mut app = test_app();
        let _ = open_agent_creator(&mut app);
        assert!(matches!(app.overlay, Overlay::AgentCreate(_)));
        // A name with a slash is rejected; the form stays open with an error.
        type_str(&mut app, "bad/name");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(effects.is_empty());
        match &app.overlay {
            Overlay::AgentCreate(form) => assert!(form.error.is_some()),
            _ => panic!("expected the create form to stay open"),
        }
    }

    #[test]
    fn create_agent_form_creates_and_switches() {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::settings::test_env::with_home(dir.path(), || {
            let mut app = test_app();
            // The create path resolves the registry base from `app.base_home`.
            app.base_home = dir.path().to_path_buf();
            let _ = open_agent_creator(&mut app);
            type_str(&mut app, "scribe");
            // Tab to the description field and add a description.
            update(&mut app, key(KeyCode::Tab));
            type_str(&mut app, "writes things");
            let effects = update(&mut app, key(KeyCode::Enter));
            // Creation succeeds → it offers to switch to the new agent.
            match effects.as_slice() {
                [Effect::SwitchAgent(name)] => assert_eq!(name, "scribe"),
                other => panic!("expected SwitchAgent, got {} effects", other.len()),
            }
            // The agent dir exists and appears in the registry list.
            let base = dir.path();
            assert!(crate::agent::registry::agent_exists(base, "scribe"));
            let names: Vec<_> = crate::agent::registry::list_agents(base)
                .into_iter()
                .map(|info| info.name)
                .collect();
            assert!(names.contains(&"scribe".to_string()));
        });
    }
}
