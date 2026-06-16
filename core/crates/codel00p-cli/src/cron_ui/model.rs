//! Pure state and transitions for the `codel00p cron` dialog.
//!
//! All state changes happen in [`CronModel::update`], which maps a key event to a
//! [`Flow`]; the driver in the parent module performs the effects ([`Flow::Reload`]
//! lists jobs, [`Flow::OpenDetail`] loads a job, [`Flow::Mutate`] writes the store,
//! [`Flow::RunNow`] executes a job). The model never touches the store or the
//! agent, so it is fully testable by feeding synthetic key events.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::composer::Composer;
use crate::tui::picker::{Picker, PickerItem, PickerOutcome};

/// A scheduled job projected for the list picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CronRow {
    pub(crate) id: String,
    /// Human-readable schedule (e.g. `every 30m`), or `invalid:<spec>`.
    pub(crate) schedule: String,
    pub(crate) enabled: bool,
    /// One-line summary of the action (prompt, or `$ codel00p ...` for commands).
    pub(crate) action: String,
    /// The raw schedule spec, the prompt, run overrides, and last-run epoch — the
    /// fields the detail screen renders.
    pub(crate) detail: JobDetail,
}

/// The fields shown on the detail screen for one job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JobDetail {
    pub(crate) schedule_spec: String,
    pub(crate) prompt: String,
    pub(crate) command: Option<Vec<String>>,
    pub(crate) workspace: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) last_run_epoch: Option<u64>,
}

impl PickerItem for CronRow {
    fn label(&self) -> String {
        format!("{}  {}", self.id, self.action)
    }
    fn detail(&self) -> Option<String> {
        let state = if self.enabled { "on" } else { "off" };
        Some(format!("{} · {state}", self.schedule))
    }
}

/// A store effect for the driver to apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mutation {
    SetEnabled { id: String, enabled: bool },
    Delete { id: String },
    Add { schedule: String, prompt: String },
}

/// Which field the create composer is currently gathering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreateStep {
    Schedule,
    Prompt,
}

/// The screen currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Detail,
    Create,
}

/// What the driver should do after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Stay,
    OpenDetail(String),
    Mutate(Mutation),
    RunNow(String),
    Quit,
}

pub(crate) struct CronModel {
    pub(crate) picker: Picker<CronRow>,
    pub(crate) screen: Screen,
    pub(crate) selected: Option<CronRow>,
    pub(crate) composer: Composer,
    /// Set while [`Screen::Create`]: which field is being gathered, and the
    /// schedule captured in the first step.
    pub(crate) create_step: CreateStep,
    pub(crate) create_schedule: String,
    pub(crate) status: Option<String>,
}

impl Default for CronModel {
    fn default() -> Self {
        CronModel {
            picker: Picker::new(Vec::new()),
            screen: Screen::List,
            selected: None,
            composer: Composer::default(),
            create_step: CreateStep::Schedule,
            create_schedule: String::new(),
            status: None,
        }
    }
}

impl CronModel {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Replaces the list rows (called by the driver on load/reload).
    pub(crate) fn set_rows(&mut self, rows: Vec<CronRow>) {
        self.picker.set_items(rows);
    }

    /// Opens the detail screen for a job (driver-loaded).
    pub(crate) fn show_detail(&mut self, row: CronRow) {
        self.selected = Some(row);
        self.screen = Screen::Detail;
    }

    /// Sets a transient status line (e.g. after a successful mutation).
    pub(crate) fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    pub(crate) fn update(&mut self, key: KeyEvent) -> Flow {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Flow::Quit;
        }
        // Any keypress clears a stale status message before acting.
        self.status = None;
        match self.screen {
            Screen::List => self.update_list(key),
            Screen::Detail => self.update_detail(key),
            Screen::Create => self.update_create(key),
        }
    }

    fn update_list(&mut self, key: KeyEvent) -> Flow {
        // `n` starts a new job. The picker also filters on typed characters, but
        // `n` is reserved here for create — filtering still works for other keys.
        if key.code == KeyCode::Char('n') && key.modifiers.is_empty() {
            return self.open_create();
        }
        match self.picker.on_key(key) {
            PickerOutcome::Selected => match self.picker.selected_item().cloned() {
                Some(row) => Flow::OpenDetail(row.id),
                None => Flow::Stay,
            },
            PickerOutcome::Cancelled => Flow::Quit,
            PickerOutcome::Pending => Flow::Stay,
        }
    }

    fn update_detail(&mut self, key: KeyEvent) -> Flow {
        let Some(row) = self.selected.clone() else {
            self.screen = Screen::List;
            return Flow::Stay;
        };
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::List;
                Flow::Stay
            }
            KeyCode::Char('e') => Flow::Mutate(Mutation::SetEnabled {
                id: row.id,
                enabled: true,
            }),
            KeyCode::Char('d') => Flow::Mutate(Mutation::SetEnabled {
                id: row.id,
                enabled: false,
            }),
            KeyCode::Char('R') => Flow::RunNow(row.id),
            KeyCode::Char('x') => Flow::Mutate(Mutation::Delete { id: row.id }),
            _ => Flow::Stay,
        }
    }

    fn open_create(&mut self) -> Flow {
        self.screen = Screen::Create;
        self.create_step = CreateStep::Schedule;
        self.create_schedule.clear();
        self.composer.clear();
        Flow::Stay
    }

    fn update_create(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Esc => {
                self.cancel_create();
                Flow::Stay
            }
            KeyCode::Enter => self.advance_create(),
            KeyCode::Backspace => {
                self.composer.backspace();
                Flow::Stay
            }
            KeyCode::Left => {
                self.composer.left();
                Flow::Stay
            }
            KeyCode::Right => {
                self.composer.right();
                Flow::Stay
            }
            KeyCode::Char(c) => {
                self.composer.insert_char(c);
                Flow::Stay
            }
            _ => Flow::Stay,
        }
    }

    fn cancel_create(&mut self) {
        self.screen = Screen::List;
        self.create_step = CreateStep::Schedule;
        self.create_schedule.clear();
        self.composer.clear();
    }

    fn advance_create(&mut self) -> Flow {
        let text = self.composer.text().trim().to_string();
        if text.is_empty() {
            self.set_status("Enter a value, or press Esc to cancel.");
            return Flow::Stay;
        }
        match self.create_step {
            CreateStep::Schedule => {
                self.create_schedule = text;
                self.create_step = CreateStep::Prompt;
                self.composer.clear();
                Flow::Stay
            }
            CreateStep::Prompt => {
                let schedule = std::mem::take(&mut self.create_schedule);
                self.composer.clear();
                self.create_step = CreateStep::Schedule;
                Flow::Mutate(Mutation::Add {
                    schedule,
                    prompt: text,
                })
            }
        }
    }
}
