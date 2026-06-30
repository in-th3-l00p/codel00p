//! Settings overlays: the main preferences list, the advanced numeric/boolean
//! knobs, the profile switcher, and the self-update prompt.
//!
//! Each key handler flips `App` state and persists the change to the user config
//! through `persist_setting`, so an in-session toggle always takes effect even if
//! the write fails. `super::update` dispatches the overlay keys here.

use super::*;

/// Opens the Settings overlay (TUI preferences such as advanced-info display).
pub(super) fn open_settings(app: &mut App) -> Vec<Effect> {
    app.overlay = Overlay::Settings(SettingsOverlay::new());
    Vec::new()
}

/// Handles keys in the Settings overlay: Up/Down move, Enter/Space toggle the
/// highlighted preference (flipping app state and persisting it to the user
/// config) or open the Advanced sub-overlay, Esc closes. Any other key leaves
/// the overlay open.
pub(super) fn handle_settings_key(
    app: &mut App,
    mut settings: SettingsOverlay,
    key: KeyEvent,
) -> Vec<Effect> {
    // API-key entry mode captures keystrokes until Enter (save) or Esc (cancel).
    if settings.api_key_entry.is_some() {
        return handle_api_key_entry(app, settings, key);
    }

    match key.code {
        KeyCode::Esc => return Vec::new(), // close
        KeyCode::Up => settings.up(),
        KeyCode::Down => settings.down(),
        // Left/Right cycle the choosers (profile, permission mode, theme); inert
        // elsewhere.
        KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => match settings.current() {
            SettingsRow::Profile => cycle_profile(app, true),
            SettingsRow::PermissionMode => cycle_permission_mode(app, true),
            SettingsRow::Theme => cycle_theme(app, true),
            _ => {}
        },
        KeyCode::Left | KeyCode::Char('-') | KeyCode::Char('_') => match settings.current() {
            SettingsRow::Profile => cycle_profile(app, false),
            SettingsRow::PermissionMode => cycle_permission_mode(app, false),
            SettingsRow::Theme => cycle_theme(app, false),
            _ => {}
        },
        KeyCode::Enter | KeyCode::Char(' ') => match settings.current() {
            SettingsRow::Pref(pref) => toggle_setting(app, pref),
            SettingsRow::PermissionMode => cycle_permission_mode(app, true),
            SettingsRow::Profile => cycle_profile(app, true),
            SettingsRow::Theme => cycle_theme(app, true),
            SettingsRow::ApiKey => {
                // Begin capturing the key for the active provider.
                settings.api_key_entry = Some(String::new());
            }
            SettingsRow::Account => show_account(app),
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

/// Handles the masked API-key entry: printable chars / Backspace edit the buffer,
/// Enter saves the key for the active provider (to the home `.env`), Esc cancels.
fn handle_api_key_entry(
    app: &mut App,
    mut settings: SettingsOverlay,
    key: KeyEvent,
) -> Vec<Effect> {
    let Some(buffer) = settings.api_key_entry.as_mut() else {
        app.overlay = Overlay::Settings(settings);
        return Vec::new();
    };
    match key.code {
        KeyCode::Esc => {
            settings.api_key_entry = None;
            app.overlay = Overlay::Settings(settings);
            Vec::new()
        }
        KeyCode::Enter => {
            let key_value = buffer.trim().to_string();
            settings.api_key_entry = None;
            app.overlay = Overlay::Settings(settings);
            if key_value.is_empty() {
                app.conversation
                    .push_notice("No key entered — nothing saved.");
                return Vec::new();
            }
            vec![Effect::SetProviderKey {
                provider: app.options.provider.clone(),
                key: key_value,
            }]
        }
        KeyCode::Backspace => {
            buffer.pop();
            app.overlay = Overlay::Settings(settings);
            Vec::new()
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            buffer.push(c);
            app.overlay = Overlay::Settings(settings);
            Vec::new()
        }
        _ => {
            app.overlay = Overlay::Settings(settings);
            Vec::new()
        }
    }
}

/// Cycles the tool-approval mode (`agent.permission_mode`) among allow/ask/deny and
/// persists it. The harness reads the same key when it builds the next turn.
fn cycle_permission_mode(app: &mut App, forward: bool) {
    const MODES: [&str; 3] = ["allow", "ask", "deny"];
    let current = app
        .permission_mode
        .as_deref()
        .and_then(|mode| MODES.iter().position(|m| *m == mode));
    let next_index = match current {
        Some(index) if forward => (index + 1) % MODES.len(),
        Some(index) => (index + MODES.len() - 1) % MODES.len(),
        None if forward => 0,
        None => MODES.len() - 1,
    };
    let next = MODES[next_index].to_string();
    app.permission_mode = Some(next.clone());
    persist_setting(
        app,
        "agent.permission_mode",
        &next,
        &format!("Tool approvals set to “{next}”. Takes effect next turn."),
        "tool approvals",
    );
}

/// Cycles the color theme (`tui.theme`) to the next (or previous) named theme,
/// applies it live to `app.theme`, and persists the token. The change is visible
/// immediately — the next render uses the new palette.
fn cycle_theme(app: &mut App, forward: bool) {
    use crate::tui::theme::Theme;
    let next = app.theme.kind.cycle(forward);
    app.theme = Theme::named(next);
    persist_setting(
        app,
        "tui.theme",
        next.name(),
        &format!("Theme set to {}.", next.label()),
        "theme",
    );
}

/// Surfaces the signed-in account as a notice (read-only). When not authenticated,
/// explains how to sign in.
fn show_account(app: &mut App) {
    match &app.cloud.viewer {
        Some(viewer) => {
            let email = viewer.email().unwrap_or("(no email)");
            let org = viewer
                .org()
                .map(|org| org.name().to_string())
                .unwrap_or_else(|| "(personal)".to_string());
            let role = viewer
                .org_role()
                .map(|role| format!("{role:?}"))
                .unwrap_or_else(|| "—".to_string());
            app.conversation
                .push_notice(format!("Signed in as {email} · org {org} · role {role}."));
        }
        None => app
            .conversation
            .push_notice("Not signed in. Run `codel00p auth login` to connect an account."),
    }
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
pub(super) fn handle_advanced_settings_key(
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
pub(super) fn handle_update_prompt_key(
    app: &mut App,
    prompt: UpdatePrompt,
    key: KeyEvent,
) -> Vec<Effect> {
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
