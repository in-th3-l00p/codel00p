//! The full-screen `codel00p config` dialog.
//!
//! `codel00p config` opens the menu; `codel00p config providers` jumps to the
//! Providers section. The pure model and rendering live in [`model`] and [`view`];
//! the terminal lifecycle and blocking loop are shared via [`crate::dialog`].
//! Callers gate on an interactive terminal — the non-TTY paths keep the scriptable
//! `config`/`providers` subcommands.

mod model;
#[cfg(test)]
mod tests;
mod view;

pub(crate) use model::Section;

use std::path::Path;

use crate::config::CliResult;
use model::{ConfigModel, Flow};

/// Runs the config dialog on the given section. Returns the saved-summary, or an
/// "unchanged" notice if the user quit without saving.
pub(crate) fn run(workspace_start: &Path, section: Section) -> CliResult<String> {
    let mut model = ConfigModel::new(workspace_start, section);
    let mut outcome = Flow::Continue;
    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Continue => Ok(true),
            done => {
                outcome = done;
                Ok(false)
            }
        }
    })?;
    match outcome {
        Flow::Save => model.persist(),
        Flow::Quit | Flow::Continue => Ok("Configuration unchanged.\n".to_string()),
    }
}
