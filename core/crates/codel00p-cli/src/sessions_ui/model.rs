//! Pure state and transitions for the `codel00p sessions` browser dialog.
//!
//! State changes happen only in [`SessionsModel::update`]; the driver in the
//! parent module loads the session list and (on `OpenDetail`) a transcript. The
//! model is store-free, so it is testable by feeding synthetic key events.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::picker::{Picker, PickerItem, PickerOutcome};

/// A persisted conversation projected for the list picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionRow {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) messages: usize,
    pub(crate) events: usize,
}

impl PickerItem for SessionRow {
    fn label(&self) -> String {
        self.id.clone()
    }
    fn detail(&self) -> Option<String> {
        Some(format!(
            "{} · {} msg · {} evt",
            self.source, self.messages, self.events
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Detail,
}

/// What the driver should do after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Stay,
    OpenDetail(String),
    /// Continue the given session in the chat TUI; the driver exits the loop and
    /// the caller launches chat on this session id.
    Resume(String),
    Quit,
}

pub(crate) struct SessionsModel {
    pub(crate) picker: Picker<SessionRow>,
    pub(crate) screen: Screen,
    pub(crate) selected: Option<SessionRow>,
    pub(crate) transcript: Vec<String>,
    pub(crate) scroll: usize,
    /// When `true`, the `?` help overlay is shown and swallows all keys.
    pub(crate) show_help: bool,
}

impl SessionsModel {
    pub(crate) fn new() -> Self {
        SessionsModel {
            picker: Picker::new(Vec::new()),
            screen: Screen::List,
            selected: None,
            transcript: Vec::new(),
            scroll: 0,
            show_help: false,
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Vec<SessionRow>) {
        self.picker.set_items(rows);
    }

    /// Opens the detail screen for `row` with its (driver-loaded) transcript.
    pub(crate) fn show_detail(&mut self, row: SessionRow, transcript: Vec<String>) {
        self.selected = Some(row);
        self.transcript = transcript;
        self.scroll = 0;
        self.screen = Screen::Detail;
    }

    pub(crate) fn update(&mut self, key: KeyEvent) -> Flow {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Flow::Quit;
        }
        // While the help overlay is up, any key (incl. Esc) just closes it and the
        // underlying screen does not act.
        if self.show_help {
            self.show_help = false;
            return Flow::Stay;
        }
        if key.code == KeyCode::Char('?') {
            self.show_help = true;
            return Flow::Stay;
        }
        match self.screen {
            Screen::List => self.update_list(key),
            Screen::Detail => self.update_detail(key),
        }
    }

    fn update_list(&mut self, key: KeyEvent) -> Flow {
        // `r` resumes the selected session in the chat TUI. Checked before the
        // picker so it is not swallowed as a filter keystroke.
        if key.code == KeyCode::Char('r')
            && key.modifiers.is_empty()
            && let Some(row) = self.picker.selected_item()
        {
            return Flow::Resume(row.id.clone());
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
        match key.code {
            KeyCode::Char('r') if key.modifiers.is_empty() => {
                if let Some(row) = self.selected.as_ref() {
                    return Flow::Resume(row.id.clone());
                }
            }
            KeyCode::Esc => {
                self.screen = Screen::List;
                self.transcript.clear();
                self.scroll = 0;
            }
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down if self.scroll + 1 < self.transcript.len() => self.scroll += 1,
            _ => {}
        }
        Flow::Stay
    }
}
