//! The async driver: terminal setup/teardown, the input/message select loop, and
//! execution of the effects an update returns. This is the only impure part of the
//! TUI — correctness lives in `update` and the pure components, which are tested
//! without a terminal.

use std::io::{self, Stdout};
use std::panic;

use codel00p_harness::UserMessage;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind};
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
use super::overlay::{ModelChoice, SessionSummary};
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

    let mut app = App::new(
        config,
        options,
        mcp_servers,
        session_state,
        persisted,
        cloud_configured,
    );

    let mut terminal =
        setup_terminal().map_err(|error| format!("terminal setup failed: {error}"))?;
    let result = event_loop(&mut app, &mut terminal).await;
    restore_terminal(&mut terminal);
    result.map(|_| String::new())
}

async fn event_loop(app: &mut App, terminal: &mut Tui) -> CliResult<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let mut reader = EventStream::new();
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(120));

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
            execute_effect(app, effect, &tx);
        }
    }
    Ok(())
}

fn map_event(event: Event) -> Option<Msg> {
    match event {
        // Ignore key-release events so a single press isn't handled twice on
        // platforms that report both.
        Event::Key(key) if key.kind != KeyEventKind::Release => Some(Msg::Key(key)),
        Event::Resize(_, _) => Some(Msg::Resize),
        _ => None,
    }
}

fn execute_effect(app: &mut App, effect: Effect, tx: &UnboundedSender<Msg>) {
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
        Effect::ResumeSession(session_id) => {
            spawn_resume_session(app.config.clone(), session_id, tx.clone())
        }
    }
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
