//! The async driver: terminal setup/teardown, the input/message select loop, and
//! execution of the effects an update returns. This is the only impure part of the
//! TUI — correctness lives in `update` and the pure components, which are tested
//! without a terminal.

use std::io::{self, Stdout};
use std::panic;

use codel00p_harness::UserMessage;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::agent::{AgentRunOptions, UiBridge, build_agent_harness_with};
use crate::config::{CliConfig, CliResult};

use super::app::App;
use super::cloud_data;
use super::msg::{CloudFetch, Effect, LocalQuery, Msg};
use super::overlay::{AgentDetailData, ModelChoice, Overlay, SessionSummary};
use super::update::update;
use super::view;

type Tui = Terminal<CrosstermBackend<Stdout>>;

pub(crate) fn run(config: CliConfig, options: AgentRunOptions) -> CliResult<String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;
    runtime.block_on(async move { run_async(config, options).await })
}

async fn run_async(config: CliConfig, options: AgentRunOptions) -> CliResult<String> {
    let mut mcp_servers = crate::agent::load_mcp_servers_from_workspace(&options.workspace)?;
    mcp_servers.extend(options.mcp_servers.clone());

    // A bare chat starts a fresh conversation; resuming is explicit via --session-id.
    let session_id = match options.session_id.as_deref() {
        Some(value) => crate::config::parse_session_id(value)?,
        None => crate::config::parse_session_id(&crate::agent::fresh_chat_session_id())?,
    };
    let (session_state, persisted) = crate::agent::load_chat_session_state(&config, session_id)?;
    let cloud_configured = cloud_data::cloud_configured();

    // The TUI display preferences load once at startup. A failed settings load
    // (e.g. malformed config) falls back to defaults: advanced info hidden, the
    // update check enabled.
    let tui_settings = crate::settings::load_layered(&options.workspace)
        .ok()
        .map(|resolved| resolved.merged.tui);
    // Advanced status-bar info is hidden by default; honor the persisted
    // `tui.show_advanced` preference if the user enabled it.
    let show_advanced = tui_settings
        .as_ref()
        .and_then(|tui| tui.show_advanced)
        .unwrap_or(false);
    // The startup update check is on by default (unset); only `false` disables it.
    let check_updates = tui_settings
        .as_ref()
        .and_then(|tui| tui.check_updates)
        .unwrap_or(true);
    // Self-awareness facets default on; honor the persisted `agent.behavior.*`
    // preferences. These are display + persistence in the overlay — the harness
    // reads the same config when it builds each turn.
    let behavior = crate::settings::load_layered(&options.workspace)
        .ok()
        .map(|resolved| resolved.merged.agent.behavior)
        .unwrap_or_default();
    let self_knowledge = behavior.self_knowledge_enabled();
    let self_state = behavior.self_state_enabled();
    let base_prompt = behavior.base_prompt_enabled();
    let auto_plan = behavior.auto_plan_enabled();
    // Harness-loop numerics edited in the Advanced sub-overlay. These persist to
    // the same `agent.*` keys the harness reads, so changes take effect on the
    // next turn. Defaults mirror the harness built-ins (max 25, verify 3,
    // failure 3) when unset.
    let agent_settings = crate::settings::load_layered(&options.workspace)
        .ok()
        .map(|resolved| resolved.merged.agent)
        .unwrap_or_default();
    let max_iterations = agent_settings.max_iterations.unwrap_or(25);
    let verify_iterations = behavior.verify_iterations_value();
    let failure_budget = behavior.failure_budget_value();
    // The active agent profile and the full set of selectable profile names
    // (built-in presets ∪ user-defined `[agent.profiles.*]`) for the Settings
    // profile switcher. Falls back to no active profile + the built-in presets
    // when the config fails to load.
    // The active local agent (multi-agent personas, #13). `main` already
    // overrode `CODEL00P_HOME` to this agent's home before config loaded, so the
    // process is already scoped to it; we read the sticky pointer purely to label
    // the banner and seed the switcher's "active" mark. `None` == the base
    // (default) agent, which is exactly today's single-home behavior. We resolve
    // the TRUE base from `CODEL00P_BASE_HOME` (stashed by `main` before its
    // override) so the pointer is read from the base, not an agent's home.
    let base_home = crate::tui::agents::base_home();
    let active_agent = crate::agent::registry::active_agent(&base_home);
    let active_profile = agent_settings.profile.clone();
    let profile_names = crate::settings::load_layered(&options.workspace)
        .ok()
        .map(|resolved| resolved.merged.agent.available_profile_names())
        .unwrap_or_else(|| {
            crate::settings::builtin_profiles()
                .into_keys()
                .collect::<Vec<_>>()
        });

    let mut app = App::new(
        config,
        options,
        mcp_servers,
        session_state,
        persisted,
        cloud_configured,
        show_advanced,
        check_updates,
        self_knowledge,
        self_state,
        base_prompt,
        auto_plan,
        max_iterations,
        verify_iterations,
        failure_budget,
        active_profile,
        profile_names,
        active_agent,
        base_home,
    );
    // The tool-approval mode (set post-construction to keep it off `App::new`).
    app.permission_mode = agent_settings.permission_mode.clone();
    // The color theme: a persisted `tui.theme` token, defaulting to Dark. Set
    // post-construction so it stays off `App::new`'s signature.
    if let Some(name) = tui_settings.as_ref().and_then(|tui| tui.theme.as_deref()) {
        app.theme = super::theme::Theme::named(super::theme::ThemeKind::from_name(name));
    }

    let mut terminal =
        setup_terminal().map_err(|error| format!("terminal setup failed: {error}"))?;
    let result = event_loop(&mut app, &mut terminal).await;
    restore_terminal(&mut terminal);

    // The self-update replaces the running binary, so it runs only here — after
    // the terminal is fully restored (raw mode off, alternate screen left) and
    // the TUI loop has exited. Its outcome message is returned to the caller.
    if app.run_update_on_exit {
        return Ok(match crate::update::run(&[]) {
            Ok(message) => message,
            Err(error) => format!("Update failed: {error}\n"),
        });
    }

    result.map(|_| String::new())
}

async fn event_loop(app: &mut App, terminal: &mut Tui) -> CliResult<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(120));

    // Kick off the background update check once at startup. It is fully
    // non-blocking: the message is queued and the loop draws the first frame
    // before the spawned check ever runs, so startup stays instant. The check
    // only runs when the setting is on AND the env kill switch is unset.
    if app.check_updates && crate::update::checks_enabled() {
        let _ = tx.send(Msg::CheckUpdates);
    }

    while !app.should_quit {
        terminal
            .draw(|frame| view::render(app, frame))
            .map_err(|error| error.to_string())?;

        let msg = tokio::select! {
            maybe_event = reader.next() => match maybe_event {
                Some(Ok(event)) => map_event(event),
                _ => None,
            },
            Some(message) = rx.recv() => Some(message),
            _ = ticker.tick() => Some(Msg::Tick),
        };

        let Some(msg) = msg else { continue };
        for effect in update(app, msg) {
            execute_effect(app, terminal, effect, &tx)?;
        }
    }
    Ok(())
}

fn map_event(event: Event) -> Option<Msg> {
    match event {
        // Ignore key-release events so a single press isn't handled twice on
        // platforms that report both.
        Event::Key(key) if key.kind != KeyEventKind::Release => Some(Msg::Key(key)),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(Msg::Scroll(3)),
            MouseEventKind::ScrollDown => Some(Msg::Scroll(-3)),
            _ => None,
        },
        Event::Resize(_, _) => Some(Msg::Resize),
        _ => None,
    }
}

fn execute_effect(
    app: &mut App,
    terminal: &mut Tui,
    effect: Effect,
    tx: &UnboundedSender<Msg>,
) -> CliResult<()> {
    match effect {
        Effect::Quit => app.should_quit = true,
        Effect::SubmitTurn(prompt) => spawn_turn(app, prompt, tx.clone()),
        Effect::ResolvePermission(reply, decision) => {
            let _ = reply.send(decision);
        }
        Effect::Persist(outcome, start) => {
            if let Err(error) = crate::agent::persist_turn_outcome(
                &app.config,
                &outcome.session_state,
                &outcome.events,
                start,
            ) {
                app.conversation
                    .push_error(format!("Failed to persist session: {error}"));
            }
        }
        Effect::Cloud(fetch) => spawn_cloud(fetch, tx.clone()),
        Effect::Local(query) => spawn_local(app.config.clone(), query, tx.clone()),
        Effect::FetchModels(provider) => spawn_models(provider, tx.clone()),
        Effect::ListSessions => spawn_session_list(app.config.clone(), tx.clone()),
        Effect::ListAgents => {
            spawn_agent_list(app.base_home.clone(), app.active_agent.clone(), tx.clone())
        }
        Effect::SwitchAgent(name) => switch_agent(app, name, tx)?,
        Effect::DeleteAgent(name) => {
            // Deleting a home is a quick filesystem removal; do it inline, surface
            // the outcome as a notice, then refresh the switcher list so the row
            // disappears. The active/default agents are guarded upstream.
            match crate::agent::registry::delete_agent(&app.base_home, &name) {
                Ok(()) => {
                    let _ = tx.send(Msg::Notice(format!("Deleted agent “{name}”.")));
                }
                Err(error) => {
                    let _ = tx.send(Msg::Notice(format!("Could not delete {name}: {error}")));
                }
            }
            spawn_agent_list(app.base_home.clone(), app.active_agent.clone(), tx.clone());
        }
        Effect::LoadAgentDetail { name, is_default } => {
            spawn_load_agent_detail(app.base_home.clone(), name, is_default, tx.clone())
        }
        Effect::SaveAgentDetail {
            name,
            is_default,
            description,
            provider,
            model,
            dispatch,
            persona,
        } => spawn_save_agent_detail(
            app.base_home.clone(),
            AgentDetailWrite {
                name,
                is_default,
                description,
                provider,
                model,
                dispatch,
                persona,
            },
            tx.clone(),
        ),
        Effect::ResumeSession(session_id) => {
            spawn_resume_session(app.config.clone(), session_id, tx.clone())
        }
        Effect::EditSession {
            session_id,
            title,
            description,
        } => spawn_edit_session(
            app.config.clone(),
            session_id,
            title,
            description,
            tx.clone(),
        ),
        Effect::DeleteSession(session_id) => {
            spawn_delete_session(app.config.clone(), session_id, tx.clone())
        }
        Effect::SwitchOrg(org_id) => switch_org(app, terminal, org_id, tx)?,
        Effect::OpenUrl(url) => {
            if let Err(error) = crate::login::open_browser(&url) {
                let _ = tx.send(Msg::Notice(format!("{error} ({url})")));
            }
        }
        Effect::SetProviderKey { provider, key } => {
            let notice = match store_provider_key(&provider, &key) {
                Ok(env_var) => format!(
                    "Saved {provider} API key to ~/.codel00p/.env ({env_var}). Applies on next launch."
                ),
                Err(error) => format!("Could not save the {provider} key: {error}"),
            };
            let _ = tx.send(Msg::Notice(notice));
        }
        Effect::CheckUpdates => spawn_update_check(tx.clone()),
    }
    Ok(())
}

/// Runs a live update check off the UI task (the blocking GitHub fetch wrapped in
/// `spawn_blocking`) and, if a newer release is found, notifies this session via
/// `Msg::UpdateAvailable`. The cache is refreshed as a side effect for next time.
/// Fully non-blocking: the UI keeps rendering while this runs.
fn spawn_update_check(tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        if let Ok(Some(version)) =
            tokio::task::spawn_blocking(crate::update::fetch_newer_version).await
        {
            let _ = tx.send(Msg::UpdateAvailable(version));
        }
    });
}

fn switch_org(
    app: &mut App,
    terminal: &mut Tui,
    org_id: String,
    tx: &UnboundedSender<Msg>,
) -> CliResult<()> {
    restore_terminal(terminal);
    let result = crate::login::run_login(&["--org".to_string(), org_id]);
    *terminal = setup_terminal().map_err(|error| format!("terminal setup failed: {error}"))?;

    match result {
        Ok(output) => {
            let message = output.trim_end();
            if !message.is_empty() {
                app.conversation.push_notice(message.to_string());
            }
            app.cloud.configured = true;
            app.overlay = Overlay::None;
            spawn_cloud(CloudFetch::Viewer, tx.clone());
            spawn_cloud(CloudFetch::Orgs, tx.clone());
            spawn_cloud(CloudFetch::Projects, tx.clone());
            spawn_cloud(CloudFetch::Users, tx.clone());
        }
        Err(error) => {
            app.conversation
                .push_error(format!("Organization switch failed: {error}"));
        }
    }

    Ok(())
}

/// Live-switches the active agent (multi-agent personas, #13). Runs
/// synchronously on the UI task — the caller (`update`) only emits
/// `Effect::SwitchAgent` when idle (`!app.turn.running`), so there is no harness
/// in flight and the `CODEL00P_HOME` mutation inside `switch_config` is
/// single-threaded.
///
/// Re-points the running TUI at the new agent's home so the NEXT harness build
/// (`spawn_turn` clones `app.config`) uses the new agent's `memory.sqlite` and
/// sessions, then resets to a fresh session under the new agent and reloads the
/// session switcher to that agent's sessions.
fn switch_agent(app: &mut App, name: String, tx: &UnboundedSender<Msg>) -> CliResult<()> {
    let workspace = app.options.workspace.clone();
    let base = app.base_home.clone();
    match super::agents::switch_config(&base, Some(&name), &workspace) {
        Ok((config, active)) => {
            // 1. Re-point config (memory + sessions resolve from config.memory_db).
            app.config = config;
            app.active_agent = active;

            // 2. Reset to a fresh conversation under the new agent's home.
            let id = crate::config::parse_session_id(&crate::agent::fresh_chat_session_id())
                .unwrap_or_default();
            app.session_state = codel00p_harness::SessionState::new(id);
            app.persisted_message_count = 0;
            app.conversation = super::conversation::Conversation::default();
            app.scroll = super::app::ScrollState::default();
            app.turn = super::app::TurnStatus::default();
            app.refresh_usage();

            let label = app
                .active_agent
                .clone()
                .unwrap_or_else(|| super::agents::DEFAULT_AGENT_LABEL.to_string());
            app.conversation.push_notice(format!(
                "Switched to agent “{label}”. New turns use its memory + sessions ({}).",
                app.session_label()
            ));

            // 3. Reload the session switcher to the new agent's sessions (if open).
            spawn_session_list(app.config.clone(), tx.clone());
        }
        Err(error) => {
            app.conversation
                .push_error(format!("Could not switch agent: {error}"));
        }
    }
    Ok(())
}

/// Lists local agents (default + registry) for the agent switcher overlay. A
/// blocking registry scan run on `spawn_blocking`, like the session list.
fn spawn_agent_list(base: std::path::PathBuf, active: Option<String>, tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            super::agents::list_agent_choices(&base, active.as_deref())
        })
        .await
        .map_err(|error| error.to_string());
        let _ = tx.send(Msg::AgentList(result));
    });
}

/// Spawns the agent turn on a task so the UI keeps rendering. The turn's tokens,
/// events, and permission requests flow back over `tx`; `TurnFinished` carries the
/// final outcome (or a humanized error).
fn spawn_turn(app: &App, prompt: String, tx: UnboundedSender<Msg>) {
    let config = app.config.clone();
    let options = app.options.clone();
    let mcp_servers = app.mcp_servers.clone();
    let session_state = app.session_state.clone();
    let parent = app.session_state.session_id().clone();
    let bridge = UiBridge { tx: tx.clone() };

    tokio::spawn(async move {
        // TUI turn cancellation (Esc) is a follow-up; pass a never-cancelled signal.
        let cancel = codel00p_harness::CancelSignal::new();
        let harness = match build_agent_harness_with(
            &config,
            &options,
            &mcp_servers,
            &parent,
            Some(bridge),
            cancel,
        )
        .await
        {
            Ok(harness) => harness,
            Err(error) => {
                let _ = tx.send(Msg::TurnFinished(Err(error)));
                return;
            }
        };
        let result = harness
            .run_turn_with_state(session_state, UserMessage::new(prompt))
            .await;
        let message = match result {
            Ok(outcome) => Msg::TurnFinished(Ok(Box::new(outcome))),
            Err(error) => Msg::TurnFinished(Err(crate::error_help::humanize_provider_error(
                &error.to_string(),
                &options.provider,
                &options.model,
            ))),
        };
        let _ = tx.send(message);
    });
}

fn spawn_cloud(fetch: CloudFetch, tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        if let Ok(message) =
            tokio::task::spawn_blocking(move || cloud_data::run_cloud_fetch(fetch)).await
        {
            let _ = tx.send(message);
        }
    });
}

fn spawn_local(config: CliConfig, query: LocalQuery, tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let text = tokio::task::spawn_blocking(move || match query {
            LocalQuery::Sessions => crate::agent::chat_sessions_listing(&config),
            LocalQuery::Memory => crate::agent::chat_memory_listing(&config),
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let notice = match text {
            Ok(body) => body.trim_end().to_string(),
            Err(error) => error,
        };
        let _ = tx.send(Msg::Notice(notice));
    });
}

/// Fetches the provider's live model catalog off the UI task and feeds it back as a
/// `Msg::Models`. `list_provider_models` is async (it makes an HTTP request), so it
/// runs directly on a Tokio task rather than `spawn_blocking`.
fn spawn_models(provider: String, tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let result = crate::providers::list_provider_models(&provider)
            .await
            .map(|models| {
                models
                    .into_iter()
                    .map(|model| ModelChoice {
                        provider: model.provider,
                        model: model.model,
                        note: model.note,
                    })
                    .collect::<Vec<_>>()
            });
        let _ = tx.send(Msg::Models(result));
    });
}

/// Lists prior sessions for the switcher overlay (a blocking session-store read run
/// on `spawn_blocking`, like the cloud fetches).
fn spawn_session_list(config: CliConfig, tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            crate::agent::chat_session_summaries(&config).map(|summaries| {
                summaries
                    .into_iter()
                    .map(|summary| SessionSummary {
                        session_id: summary.session_id,
                        title: summary.title,
                        description: summary.description,
                        source: summary.source,
                        message_count: summary.message_count,
                    })
                    .collect::<Vec<_>>()
            })
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let _ = tx.send(Msg::SessionList(result));
    });
}

/// Replays a prior session so it can be resumed in place, returning the rebuilt
/// `SessionState` plus its persisted message count over `Msg::SessionResumed`.
fn spawn_resume_session(
    config: CliConfig,
    session_id: codel00p_harness::SessionId,
    tx: UnboundedSender<Msg>,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            crate::agent::load_chat_session_state(&config, session_id)
                .map(|(state, persisted)| (Box::new(state), persisted))
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let _ = tx.send(Msg::SessionResumed(result));
    });
}

/// Applies a conversation's edited name + description (blocking store writes on
/// `spawn_blocking`), then surfaces the result as a notice and refreshes the
/// switcher list so the change shows immediately. An empty description clears it.
fn spawn_edit_session(
    config: CliConfig,
    session_id: codel00p_harness::SessionId,
    title: String,
    description: String,
    tx: UnboundedSender<Msg>,
) {
    let list_config = config.clone();
    tokio::spawn(async move {
        let id = session_id.as_str().to_string();
        let title_for_msg = title.clone();
        let result = tokio::task::spawn_blocking(move || {
            use codel00p_session::SessionStore;
            let mut store = crate::config::open_session_store(&config)?;
            store
                .set_session_title(&session_id, &title)
                .map_err(|error| error.to_string())?;
            store
                .set_session_description(&session_id, &description)
                .map_err(|error| error.to_string())
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let notice = match result {
            Ok(()) => format!("Updated conversation {id} (\"{title_for_msg}\")."),
            Err(error) => format!("Could not update {id}: {error}"),
        };
        let _ = tx.send(Msg::Notice(notice));
        // Refresh the switcher so the change is reflected if it is still open.
        spawn_session_list(list_config, tx);
    });
}

/// Deletes a conversation (a blocking store write on `spawn_blocking`), then
/// surfaces the result as a notice and refreshes the switcher list so the row
/// disappears immediately.
fn spawn_delete_session(
    config: CliConfig,
    session_id: codel00p_harness::SessionId,
    tx: UnboundedSender<Msg>,
) {
    let list_config = config.clone();
    tokio::spawn(async move {
        let id = session_id.as_str().to_string();
        let result = tokio::task::spawn_blocking(move || {
            use codel00p_session::SessionStore;
            let mut store = crate::config::open_session_store(&config)?;
            store
                .delete_session(&session_id)
                .map_err(|error| error.to_string())
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let notice = match result {
            Ok(true) => format!("Deleted conversation {id}."),
            Ok(false) => format!("No such conversation: {id}."),
            Err(error) => format!("Could not delete {id}: {error}"),
        };
        let _ = tx.send(Msg::Notice(notice));
        // Refresh the switcher so the deleted row is gone if it is still open.
        spawn_session_list(list_config, tx);
    });
}

/// Resolves an agent's home directory: the base itself for the default agent, or
/// `<base>/agents/<name>` for a registry agent.
fn agent_home_dir(base: &std::path::Path, name: &str, is_default: bool) -> std::path::PathBuf {
    if is_default {
        base.to_path_buf()
    } else {
        crate::agent::registry::agent_home(base, name)
    }
}

/// Formats a byte count compactly for the memory summary line.
fn human_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Reads an agent's on-disk detail (config.toml + persona.md + agent.toml + memory
/// db size) off the UI task and delivers it over `Msg::AgentDetailLoaded`.
fn spawn_load_agent_detail(
    base: std::path::PathBuf,
    name: String,
    is_default: bool,
    tx: UnboundedSender<Msg>,
) {
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            let home = agent_home_dir(&base, &name, is_default);
            let settings = crate::settings::load_file(&home.join("config.toml"))
                .ok()
                .flatten()
                .unwrap_or_default();
            let description = if is_default {
                String::new()
            } else {
                crate::agent::registry::agent_info(&base, &name)
                    .and_then(|info| info.description)
                    .unwrap_or_default()
            };
            let persona = std::fs::read_to_string(home.join("persona.md")).unwrap_or_default();
            let memory_note = match std::fs::metadata(home.join("memory.sqlite")) {
                Ok(meta) => format!("memory.sqlite · {}", human_size(meta.len())),
                Err(_) => "no memory recorded yet".to_string(),
            };
            Ok::<_, String>(AgentDetailData {
                description,
                provider: settings.agent.provider.unwrap_or_default(),
                model: settings.agent.model.unwrap_or_default(),
                dispatch: settings
                    .agent
                    .fallbacks
                    .map(|routes| routes.join(", "))
                    .unwrap_or_default(),
                persona,
                memory_note,
            })
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let _ = tx.send(Msg::AgentDetailLoaded(result));
    });
}

/// The values to persist for an agent's detail/edit save.
struct AgentDetailWrite {
    name: String,
    is_default: bool,
    description: String,
    provider: String,
    model: String,
    dispatch: String,
    persona: String,
}

/// Persists edited agent detail off the UI task: config.toml provider/model/dispatch
/// (set when non-empty, unset when cleared), persona.md, and — for registry agents —
/// the agent.toml description. Surfaces the outcome as a notice.
fn spawn_save_agent_detail(
    base: std::path::PathBuf,
    write: AgentDetailWrite,
    tx: UnboundedSender<Msg>,
) {
    tokio::spawn(async move {
        let name_for_msg = write.name.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let home = agent_home_dir(&base, &write.name, write.is_default);
            let config = home.join("config.toml");

            // Scalar keys: set when present, unset (clear) when blank so the agent
            // falls back to inherited defaults.
            let put = |key: &str, value: &str| -> Result<(), String> {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    crate::settings::unset_value(&config, key).map(|_| ())
                } else {
                    crate::settings::set_value(&config, key, trimmed)
                }
            };
            put("agent.provider", &write.provider)?;
            put("agent.model", &write.model)?;
            put("agent.fallbacks", &write.dispatch)?;

            // Persona is a plain file under the agent's home.
            std::fs::write(home.join("persona.md"), &write.persona)
                .map_err(|error| format!("failed to write persona.md: {error}"))?;

            // Description lives in agent.toml; only registry agents have one.
            if !write.is_default {
                crate::agent::registry::set_agent_description(
                    &base,
                    &write.name,
                    Some(write.description.as_str()),
                )?;
            }
            Ok(())
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        let notice = match result {
            Ok(()) => format!("Saved agent “{name_for_msg}”."),
            Err(error) => format!("Could not save {name_for_msg}: {error}"),
        };
        let _ = tx.send(Msg::Notice(notice));
    });
}

/// Upserts a provider's API key into the active home's `.env` (the credential file
/// loaded at startup). Resolves the provider's standard env var name, then replaces
/// an existing `KEY=…` line or appends one. Returns the env var name on success.
fn store_provider_key(provider: &str, key: &str) -> Result<&'static str, String> {
    let env_var = codel00p_providers::default_registry()
        .resolve(provider)
        .and_then(|profile| profile.env_vars.last().copied())
        .ok_or_else(|| format!("unknown provider `{provider}` (no known API-key env var)"))?;

    let path = crate::settings::env_file_path();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;
    for line in existing.lines() {
        let is_match = line
            .split_once('=')
            .map(|(k, _)| k.trim() == env_var)
            .unwrap_or(false);
        if is_match {
            lines.push(format!("{env_var}={key}"));
            replaced = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !replaced {
        lines.push(format!("{env_var}={key}"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut body = lines.join("\n");
    body.push('\n');
    std::fs::write(&path, body).map_err(|error| format!("failed to write .env: {error}"))?;
    Ok(env_var)
}

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    install_panic_hook();
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) {
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
}

/// Restores the terminal before the default panic handler prints, so a panic in the
/// UI doesn't leave the user's terminal in raw mode / the alternate screen.
fn install_panic_hook() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original(info);
    }));
}
