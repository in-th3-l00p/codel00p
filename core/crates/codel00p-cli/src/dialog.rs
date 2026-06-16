//! Shared scaffolding for the synchronous full-screen dialogs (`config`,
//! `memory`, and the Phase-3 groups). It owns the terminal lifecycle and a
//! generic blocking Elm loop, so each dialog's `run` becomes: build a model, then
//! hand a `draw` and an `on_key` to [`run_blocking`].
//!
//! The async chat TUI (`tui/`) keeps its own loop — it multiplexes channel
//! messages and captures the mouse, which this synchronous helper does not.

use std::io::{self, Stdout};
use std::panic;

use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::config::CliResult;

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Runs a blocking, full-screen Elm loop over `model`, restoring the terminal on
/// every exit path (errors and the key handler returning `Ok(false)` included).
///
/// - `draw` renders the model each frame.
/// - `on_key` applies one key press and returns `Ok(true)` to keep going,
///   `Ok(false)` to quit, or `Err` to abort (the terminal is still restored).
pub(crate) fn run_blocking<M>(
    model: &mut M,
    draw: impl FnMut(&mut Frame, &M),
    on_key: impl FnMut(&mut M, KeyEvent) -> CliResult<bool>,
) -> CliResult<()> {
    let mut terminal =
        setup_terminal().map_err(|error| format!("terminal setup failed: {error}"))?;
    let result = drive(&mut terminal, model, draw, on_key);
    restore_terminal(&mut terminal);
    result
}

fn drive<M>(
    terminal: &mut Tui,
    model: &mut M,
    mut draw: impl FnMut(&mut Frame, &M),
    mut on_key: impl FnMut(&mut M, KeyEvent) -> CliResult<bool>,
) -> CliResult<()> {
    loop {
        terminal
            .draw(|frame| draw(frame, &*model))
            .map_err(|error| error.to_string())?;
        let Event::Key(key) = event::read().map_err(|error| error.to_string())? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if !on_key(model, key)? {
            return Ok(());
        }
    }
}

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    install_panic_hook();
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

/// Restores the terminal before the default panic handler prints, so a panic in a
/// dialog doesn't leave the user in raw mode / the alternate screen.
fn install_panic_hook() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}
