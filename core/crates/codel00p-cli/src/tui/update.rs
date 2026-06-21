//! The pure update loop: `update(&mut App, Msg) -> Vec<Effect>`. No terminal, no
//! network, no async — every state change in the TUI flows through here, which is
//! what makes the behavior unit-testable without a real terminal.

use codel00p_harness::{HarnessEvent, PermissionDecision, PermissionMode, SessionState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;
use super::msg::{CloudFetch, Effect, LocalQuery, Msg};
use super::overlay::{
    AdvancedKind, AdvancedPref, AdvancedSettingsOverlay, CommandAction, CommandPalette,
    EntityBrowser, EntityTab, ModelPicker, Overlay, SessionSwitcher, SettingsOverlay, SettingsPref,
    SettingsRow, UpdatePrompt,
};
use super::picker::PickerOutcome;

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
    // Ctrl-P opens the command palette from anywhere — the unified launcher for
    // every action, so users do not have to remember the individual F-keys.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
        return open_command_palette(app);
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
        Overlay::UpdatePrompt(prompt) => handle_update_prompt_key(app, prompt, key),
        Overlay::Command(mut palette) => match palette.on_key(key) {
            PickerOutcome::Selected => match palette.selected_item() {
                Some(item) => run_command(app, item.action),
                None => Vec::new(),
            },
            PickerOutcome::Cancelled => Vec::new(),
            PickerOutcome::Pending => {
                app.overlay = Overlay::Command(palette);
                Vec::new()
            }
        },
        Overlay::None => Vec::new(),
    }
}

/// Handles keys in the session switcher. In normal (list) mode, F2 enters an inline
/// rename for the highlighted row, Enter resumes it, Esc closes. In rename mode the
/// key edits the title buffer: Enter confirms (dispatching a rename), Esc cancels
/// back to the list, and printable characters / Backspace edit.
fn handle_sessions_key(app: &mut App, mut switcher: SessionSwitcher, key: KeyEvent) -> Vec<Effect> {
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

/// Opens the command palette overlay (the unified launcher).
fn open_command_palette(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::Command(CommandPalette::new());
    Vec::new()
}

/// Opens the Settings overlay (TUI preferences such as advanced-info display).
fn open_settings(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::Settings(SettingsOverlay::new());
    Vec::new()
}

/// Handles keys in the Settings overlay: Up/Down move, Enter/Space toggle the
/// highlighted preference (flipping app state and persisting it to the user
/// config) or open the Advanced sub-overlay, Esc closes. Any other key leaves
/// the overlay open.
fn handle_settings_key(app: &mut App, mut settings: SettingsOverlay, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::Esc => return Vec::new(), // close
        KeyCode::Up => settings.up(),
        KeyCode::Down => settings.down(),
        // Left/Right cycle the profile switcher (back/forward); they are inert on
        // the other rows.
        KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
            if matches!(settings.current(), SettingsRow::Profile) {
                cycle_profile(app, true);
            }
        }
        KeyCode::Left | KeyCode::Char('-') | KeyCode::Char('_') => {
            if matches!(settings.current(), SettingsRow::Profile) {
                cycle_profile(app, false);
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => match settings.current() {
            SettingsRow::Pref(pref) => toggle_setting(app, pref),
            SettingsRow::Profile => cycle_profile(app, true),
            SettingsRow::Advanced => {
                // Open the Advanced sub-overlay; Esc there returns to Settings.
                app.overlay = Overlay::AdvancedSettings(AdvancedSettingsOverlay::new());
                return Vec::new();
            }
        },
        _ => {}
    }
    app.overlay = Overlay::Settings(settings);
    Vec::new()
}

/// Cycles the active agent profile to the next (or previous) name in
/// `app.profile_names`, persists it to `agent.profile`, and updates in-memory
/// state. When no profile is active yet, the first cycle selects the first name.
/// A no-op (with a notice) when no profiles are available.
fn cycle_profile(app: &mut App, forward: bool) {
    let names = &app.profile_names;
    if names.is_empty() {
        app.conversation
            .push_notice("No agent profiles available to switch to.".to_string());
        return;
    }
    let current = app
        .active_profile
        .as_ref()
        .and_then(|name| names.iter().position(|candidate| candidate == name));
    let next_index = match current {
        Some(index) if forward => (index + 1) % names.len(),
        Some(index) => (index + names.len() - 1) % names.len(),
        // No active profile selected yet: forward picks the first, back the last.
        None if forward => 0,
        None => names.len() - 1,
    };
    let next = names[next_index].clone();
    app.active_profile = Some(next.clone());
    persist_setting(
        app,
        "agent.profile",
        &next,
        &format!("Agent profile set to {next}. Takes effect next run."),
        "agent profile",
    );
}

/// Handles keys in the Advanced settings sub-overlay: Up/Down move, Left/Right
/// (or -/+) adjust the highlighted numeric knob within its bounds, Enter/Space
/// toggle a boolean knob, Esc returns to the main Settings overlay. Every change
/// is persisted immediately to the config key the harness reads.
fn handle_advanced_settings_key(
    app: &mut App,
    mut advanced: AdvancedSettingsOverlay,
    key: KeyEvent,
) -> Vec<Effect> {
    match key.code {
        KeyCode::Esc => {
            // Return to the main Settings overlay rather than closing outright.
            app.overlay = Overlay::Settings(SettingsOverlay::new());
            return Vec::new();
        }
        KeyCode::Up => advanced.up(),
        KeyCode::Down => advanced.down(),
        KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
            adjust_advanced(app, advanced.current(), true);
        }
        KeyCode::Left | KeyCode::Char('-') | KeyCode::Char('_') => {
            adjust_advanced(app, advanced.current(), false);
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            toggle_advanced(app, advanced.current());
        }
        _ => {}
    }
    app.overlay = Overlay::AdvancedSettings(advanced);
    Vec::new()
}

/// Reads the current value of a numeric advanced preference from app state.
fn advanced_number(app: &App, pref: AdvancedPref) -> u32 {
    match pref {
        AdvancedPref::MaxIterations => app.max_iterations,
        AdvancedPref::VerifyIterations => app.verify_iterations,
        AdvancedPref::FailureBudget => app.failure_budget,
        _ => 0,
    }
}

/// Increments or decrements a numeric advanced knob by its step, clamped to its
/// bounds, then persists the new value. No-op on boolean prefs.
fn adjust_advanced(app: &mut App, pref: AdvancedPref, increase: bool) {
    let AdvancedKind::Number { step, min, max } = pref.kind() else {
        return;
    };
    let current = advanced_number(app, pref);
    let next = if increase {
        current.saturating_add(step).min(max)
    } else {
        current.saturating_sub(step).max(min)
    };
    if next == current {
        return; // already at the bound — nothing to write
    }
    match pref {
        AdvancedPref::MaxIterations => app.max_iterations = next,
        AdvancedPref::VerifyIterations => app.verify_iterations = next,
        AdvancedPref::FailureBudget => app.failure_budget = next,
        _ => {}
    }
    persist_setting(
        app,
        pref.config_key(),
        &next.to_string(),
        &format!("{} set to {next}.", pref.label()),
        pref.label(),
    );
}

/// Toggles a boolean advanced preference, persisting the new value. No-op on
/// numeric prefs.
fn toggle_advanced(app: &mut App, pref: AdvancedPref) {
    if !matches!(pref.kind(), AdvancedKind::Bool) {
        return;
    }
    let new_value = match pref {
        AdvancedPref::SelfKnowledge => {
            app.self_knowledge = !app.self_knowledge;
            app.self_knowledge
        }
        AdvancedPref::SelfState => {
            app.self_state = !app.self_state;
            app.self_state
        }
        AdvancedPref::BasePrompt => {
            app.base_prompt = !app.base_prompt;
            app.base_prompt
        }
        AdvancedPref::AutoPlan => {
            app.auto_plan = !app.auto_plan;
            app.auto_plan
        }
        _ => return,
    };
    let state = if new_value { "enabled" } else { "disabled" };
    persist_setting(
        app,
        pref.config_key(),
        if new_value { "true" } else { "false" },
        &format!("{} {state}.", pref.label()),
        pref.label(),
    );
}

/// Flips the given preference in app state and persists the new value to the
/// user config. A failed write is surfaced as a notice but the in-session toggle
/// still takes effect.
fn toggle_setting(app: &mut App, pref: SettingsPref) {
    match pref {
        SettingsPref::ShowAdvanced => {
            app.show_advanced = !app.show_advanced;
            let value = if app.show_advanced { "true" } else { "false" };
            persist_setting(
                app,
                "tui.show_advanced",
                value,
                &format!(
                    "Advanced info {}.",
                    if app.show_advanced { "shown" } else { "hidden" }
                ),
                "advanced info",
            );
        }
        SettingsPref::CheckUpdates => {
            app.check_updates = !app.check_updates;
            let value = if app.check_updates { "true" } else { "false" };
            persist_setting(
                app,
                "tui.check_updates",
                value,
                &format!(
                    "Update checks {}.",
                    if app.check_updates {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                "update checks",
            );
        }
    }
}

/// Persists a single dotted setting to the user config, surfacing success or a
/// failed write as a conversation notice. The in-session toggle takes effect
/// regardless of whether the write succeeds.
fn persist_setting(app: &mut App, key: &str, value: &str, ok_message: &str, what: &str) {
    let path = crate::settings::user_config_path();
    match crate::settings::set_value(&path, key, value) {
        Ok(()) => app.conversation.push_notice(ok_message.to_string()),
        Err(error) => app
            .conversation
            .push_notice(format!("Toggled {what}, but could not save it: {error}")),
    }
}

/// Handles keys in the update-prompt panel: Enter runs the self-update (queued to
/// run after the terminal is restored, then quits), Esc dismisses (closing the
/// panel and marking it dismissed so it does not reopen this session). Any other
/// key leaves the panel open.
fn handle_update_prompt_key(app: &mut App, prompt: UpdatePrompt, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::Enter => {
            // The self-update replaces the running binary, so it must not run while
            // the TUI owns the terminal: flag it and quit; the event loop restores
            // the terminal first, then runs the update.
            app.run_update_on_exit = true;
            vec![Effect::Quit]
        }
        KeyCode::Esc => {
            app.update_prompt_dismissed = true;
            Vec::new() // close the overlay; the header chip keeps it discoverable
        }
        _ => {
            app.overlay = Overlay::UpdatePrompt(prompt);
            Vec::new()
        }
    }
}

/// Dispatches a palette selection to the existing action handlers.
fn run_command(app: &mut App, action: CommandAction) -> Vec<Effect> {
    match action {
        CommandAction::Model => open_model_picker(app),
        CommandAction::Sessions => open_sessions(app),
        CommandAction::NewConversation => handle_slash(app, "reset"),
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
        EntityTab::Org => match browser.orgs.on_key(key) {
            PickerOutcome::Selected => {
                if let Some(org) = browser.orgs.selected_item() {
                    let org_id = org.id().to_string();
                    if app
                        .cloud
                        .viewer
                        .as_ref()
                        .and_then(|viewer| viewer.org())
                        .map(|active| active.id() == org_id)
                        .unwrap_or(false)
                    {
                        app.conversation
                            .push_notice(format!("Already using organization {}.", org.name()));
                        return Vec::new();
                    }
                    app.conversation.push_notice(format!(
                        "Opening browser to switch to organization {}…",
                        org.name()
                    ));
                    return vec![Effect::SwitchOrg(org_id)];
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
fn scroll_up(app: &mut App, rows: u16) {
    app.scroll.follow = false;
    app.scroll.offset_from_bottom = app.scroll.offset_from_bottom.saturating_add(rows);
}

/// Scrolls the transcript down (toward newer messages). Reaching the bottom
/// re-enables follow mode so new content keeps tailing.
fn scroll_down(app: &mut App, rows: u16) {
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
    app.scroll = super::app::ScrollState::default();
    app.turn = super::app::TurnStatus {
        running: true,
        started_tick: app.tick,
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
            app.scroll = super::app::ScrollState::default();
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
        Effect::Cloud(CloudFetch::Orgs),
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
        InferenceCompleted {
            finish_reason,
            usage,
            cost,
            ..
        } => {
            app.turn.finish_reason = finish_reason.clone();
            // Capture real provider token usage when reported; this is preferred
            // over the char-count estimate in the advanced status bar.
            if let Some(usage) = usage {
                app.last_usage = Some(usage.clone());
            }
            // Capture the cost estimate when the provider priced the call. Left
            // untouched when absent so the HUD never shows a bogus `$0.00`.
            if let Some(cost) = cost {
                app.last_cost = Some(cost.clone());
            }
        }
        TurnCompleted {
            iterations,
            usage,
            cost,
            ..
        } => {
            app.turn.iterations = *iterations;
            // The turn-total usage supersedes the last per-inference figure.
            if let Some(usage) = usage {
                app.last_usage = Some(usage.clone());
            }
            if let Some(cost) = cost {
                app.last_cost = Some(cost.clone());
            }
        }
        // The verify-before-done loop ran the project's checks and reached a
        // verdict — surface it as a transcript line and a persistent status-bar
        // signal so automated verification is visible (a trust signal).
        VerificationCompleted {
            check,
            command,
            success,
            ..
        } => {
            if *success {
                let line = format!("✓ Verified: {check} pass");
                app.verification = Some(line.clone());
                app.conversation.push_notice(line);
            } else {
                let line = format!("⚠ Verification failed: `{command}` — retrying");
                app.verification = Some(format!("⚠ {check} failed — retrying"));
                app.conversation.push_notice(line);
            }
        }
        // The self-critique reflection step ran. If it produced more tool calls
        // the loop continued (a gap was addressed); otherwise it confirmed done.
        SelfCritiqueCompleted {
            produced_tool_calls,
            ..
        } => {
            let line = if *produced_tool_calls {
                "○ Self-check found a gap — addressing it".to_string()
            } else {
                "○ Self-check done".to_string()
            };
            app.verification = Some(line.clone());
            app.conversation.push_notice(line);
        }
        // The in-turn failure budget was hit: the same operation kept failing and
        // a step-back/replan nudge was injected. Surface it as a visible signal.
        FailureBudgetExceeded {
            operation,
            attempts,
            ..
        } => {
            let line = format!("⚠ repeated failures ({operation} ×{attempts}) — replanning");
            app.verification = Some("⚠ repeated failures — replanning".to_string());
            app.conversation.push_notice(line);
        }
        // `ToolCallArgsDelta` reaches the bridge here (via `ChannelEventSink`)
        // but is intentionally not rendered into the transcript: it is a live
        // signal, and the assembled call is surfaced by `ToolCallRequested`
        // below it. Other consumers (e.g. `--stream-events`) observe the delta.
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

    #[test]
    fn session_list_populates_switcher_and_selecting_resumes() {
        use crate::tui::overlay::SessionSummary;
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(5)));
        update(
            &mut app,
            Msg::SessionList(Ok(vec![SessionSummary {
                session_id: "chat-42".to_string(),
                title: Some("Fix switcher history".to_string()),
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
    fn f2_in_switcher_enters_rename_and_enter_produces_rename_effect() {
        use crate::tui::overlay::SessionSummary;
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(5)));
        update(
            &mut app,
            Msg::SessionList(Ok(vec![SessionSummary {
                session_id: "chat-7".to_string(),
                title: Some("Old name".to_string()),
                source: "cli".to_string(),
                message_count: 1,
            }])),
        );
        // F2 enters rename mode seeded with the current title.
        update(&mut app, key(KeyCode::F(2)));
        match &app.overlay {
            Overlay::Sessions(switcher) => {
                let rename = switcher.rename.as_ref().expect("rename mode");
                assert_eq!(rename.session_id, "chat-7");
                assert_eq!(rename.input, "Old name");
            }
            _ => panic!("expected session switcher"),
        }
        // Clear it and type a new title.
        for _ in 0.."Old name".len() {
            update(&mut app, key(KeyCode::Backspace));
        }
        type_str(&mut app, "Fresh title");
        let effects = update(&mut app, key(KeyCode::Enter));
        match effects.as_slice() {
            [Effect::RenameSession(id, title)] => {
                assert_eq!(id.as_str(), "chat-7");
                assert_eq!(title, "Fresh title");
            }
            other => panic!("expected RenameSession, got {} effects", other.len()),
        }
        // The switcher stays open (rename cleared) so the refreshed list can land.
        assert!(matches!(app.overlay, Overlay::Sessions(_)));
    }

    #[test]
    fn esc_cancels_rename_without_leaving_the_switcher() {
        use crate::tui::overlay::SessionSummary;
        let mut app = test_app();
        update(&mut app, key(KeyCode::F(5)));
        update(
            &mut app,
            Msg::SessionList(Ok(vec![SessionSummary {
                session_id: "chat-7".to_string(),
                title: Some("Keep me".to_string()),
                source: "cli".to_string(),
                message_count: 1,
            }])),
        );
        update(&mut app, key(KeyCode::F(2)));
        let effects = update(&mut app, key(KeyCode::Esc));
        assert!(effects.is_empty());
        match &app.overlay {
            Overlay::Sessions(switcher) => assert!(switcher.rename.is_none()),
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
    fn ctrl_p_opens_the_command_palette() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        assert!(matches!(app.overlay, Overlay::Command(_)));
    }

    #[test]
    fn selecting_a_command_runs_its_action() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        // The first palette row is "Switch model"; Enter runs it.
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(app.overlay, Overlay::Model(_)));
        assert!(matches!(effects.as_slice(), [Effect::FetchModels(_)]));
    }

    #[test]
    fn command_palette_filters_and_quit_runs() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        // Filter down to "Quit" and run it.
        type_str(&mut app, "quit");
        let effects = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
    }

    #[test]
    fn settings_palette_action_opens_settings_overlay() {
        let mut app = test_app();
        update(&mut app, ctrl(KeyCode::Char('p')));
        // Filter down to "Settings" and run it.
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
}
