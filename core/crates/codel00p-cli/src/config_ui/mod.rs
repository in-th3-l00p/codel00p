//! The full-screen `codel00p config` dialog.
//!
//! `codel00p config` opens the menu; `codel00p config providers` jumps to the
//! Providers section. The pure model and rendering live in [`model`] and [`view`];
//! this module owns the terminal lifecycle and the blocking event loop. Callers
//! gate on an interactive terminal — the non-TTY paths keep the scriptable
//! `config`/`providers` subcommands.

mod model;
#[cfg(test)]
mod tests;
mod view;

pub(crate) use model::Section;

use std::io::{self, Stdout};
use std::path::Path;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::config::CliResult;
use model::{ConfigModel, Flow};

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Runs the config dialog on the given section, restoring the terminal on every
/// exit path. Returns the saved-summary, or an "unchanged" notice if the user
/// quit without saving.
pub(crate) fn run(workspace_start: &Path, section: Section) -> CliResult<String> {
    let mut model = ConfigModel::new(workspace_start, section);
    let mut terminal =
        setup_terminal().map_err(|error| format!("terminal setup failed: {error}"))?;
    let flow = event_loop(&mut model, &mut terminal);
    restore_terminal(&mut terminal);
    match flow? {
        Flow::Save => model.persist(),
        Flow::Quit | Flow::Continue => Ok("Configuration unchanged.\n".to_string()),
    }
}

fn event_loop(model: &mut ConfigModel, terminal: &mut Tui) -> CliResult<Flow> {
    loop {
        terminal
            .draw(|frame| view::draw(frame, model))
            .map_err(|error| error.to_string())?;
        if let Event::Key(key) = event::read().map_err(|error| error.to_string())? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match model.update(key) {
                Flow::Continue => {}
                done => return Ok(done),
            }
        }
    }
}

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Tui) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}
