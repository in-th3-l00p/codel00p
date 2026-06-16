//! Pure state and transitions for the `codel00p memory` review dialog.
//!
//! All state changes happen in [`MemoryModel::update`], which maps a key event to
//! a [`Flow`]; the driver in the parent module performs the effects ([`Flow::Reload`]
//! lists, [`Flow::OpenDetail`] loads an audit trail, [`Flow::Mutate`] calls the
//! repository). The model never touches the store, so it is fully testable by
//! feeding synthetic key events.

use codel00p_protocol::{MemoryKind, MemoryStatus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::composer::Composer;
use crate::tui::picker::{Picker, PickerItem, PickerOutcome};

/// A memory record projected for the list picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemoryRow {
    pub(crate) id: String,
    pub(crate) status: MemoryStatus,
    pub(crate) kind: MemoryKind,
    pub(crate) content: String,
    pub(crate) tags: Vec<String>,
}

impl PickerItem for MemoryRow {
    fn label(&self) -> String {
        self.content.clone()
    }
    fn detail(&self) -> Option<String> {
        Some(format!(
            "{} · {}",
            status_label(self.status),
            kind_label(self.kind)
        ))
    }
}

/// One row of a record's audit trail, shown on the detail screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuditRow {
    pub(crate) sequence: u64,
    pub(crate) action: String,
    pub(crate) actor: String,
    pub(crate) reason: Option<String>,
}

/// The status the list is filtered to. `All` shows every status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusFilter {
    Candidate,
    Approved,
    Rejected,
    Archived,
    All,
}

impl StatusFilter {
    pub(crate) const ORDER: [StatusFilter; 5] = [
        StatusFilter::Candidate,
        StatusFilter::Approved,
        StatusFilter::Rejected,
        StatusFilter::Archived,
        StatusFilter::All,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            StatusFilter::Candidate => "Candidate",
            StatusFilter::Approved => "Approved",
            StatusFilter::Rejected => "Rejected",
            StatusFilter::Archived => "Archived",
            StatusFilter::All => "All",
        }
    }

    /// The status to filter the repository query by, or `None` for `All`.
    pub(crate) fn status(self) -> Option<MemoryStatus> {
        match self {
            StatusFilter::Candidate => Some(MemoryStatus::Candidate),
            StatusFilter::Approved => Some(MemoryStatus::Approved),
            StatusFilter::Rejected => Some(MemoryStatus::Rejected),
            StatusFilter::Archived => Some(MemoryStatus::Archived),
            StatusFilter::All => None,
        }
    }

    fn next(self) -> StatusFilter {
        let index = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        Self::ORDER[(index + 1) % Self::ORDER.len()]
    }

    fn prev(self) -> StatusFilter {
        let index = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        Self::ORDER[(index + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// A prompt-gathered review action awaiting confirmation in the composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingAction {
    Reject,
    Archive,
    Edit,
}

/// A review effect for the driver to apply against the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mutation {
    Approve { id: String },
    Reject { id: String, reason: String },
    Archive { id: String, reason: String },
    Edit { id: String, content: String },
}

/// The screen currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Detail,
    Prompt,
}

/// What the driver should do after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Stay,
    Reload,
    OpenDetail(String),
    Mutate(Mutation),
    Quit,
}

pub(crate) struct MemoryModel {
    pub(crate) filter: StatusFilter,
    pub(crate) picker: Picker<MemoryRow>,
    pub(crate) screen: Screen,
    pub(crate) selected: Option<MemoryRow>,
    pub(crate) detail_audit: Vec<AuditRow>,
    pub(crate) pending: Option<PendingAction>,
    pub(crate) composer: Composer,
    pub(crate) actor: String,
    pub(crate) status: Option<String>,
}

impl MemoryModel {
    pub(crate) fn new(actor: String) -> Self {
        MemoryModel {
            filter: StatusFilter::Candidate,
            picker: Picker::new(Vec::new()),
            screen: Screen::List,
            selected: None,
            detail_audit: Vec::new(),
            pending: None,
            composer: Composer::default(),
            actor,
            status: None,
        }
    }

    /// Replaces the list rows (called by the driver on load/reload).
    pub(crate) fn set_rows(&mut self, rows: Vec<MemoryRow>) {
        self.picker.set_items(rows);
    }

    /// Opens the detail screen with a record and its audit trail (driver-loaded).
    pub(crate) fn show_detail(&mut self, row: MemoryRow, audit: Vec<AuditRow>) {
        self.selected = Some(row);
        self.detail_audit = audit;
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
        match self.screen {
            Screen::List => self.update_list(key),
            Screen::Detail => self.update_detail(key),
            Screen::Prompt => self.update_prompt(key),
        }
    }

    fn update_list(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Tab => {
                self.filter = self.filter.next();
                Flow::Reload
            }
            KeyCode::BackTab => {
                self.filter = self.filter.prev();
                Flow::Reload
            }
            _ => match self.picker.on_key(key) {
                PickerOutcome::Selected => match self.picker.selected_item().cloned() {
                    Some(row) => Flow::OpenDetail(row.id),
                    None => Flow::Stay,
                },
                PickerOutcome::Cancelled => Flow::Quit,
                PickerOutcome::Pending => Flow::Stay,
            },
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
            KeyCode::Char('a') => Flow::Mutate(Mutation::Approve { id: row.id }),
            KeyCode::Char('r') => self.open_prompt(PendingAction::Reject, String::new()),
            KeyCode::Char('x') => self.open_prompt(PendingAction::Archive, String::new()),
            KeyCode::Char('e') => self.open_prompt(PendingAction::Edit, row.content),
            _ => Flow::Stay,
        }
    }

    fn open_prompt(&mut self, action: PendingAction, initial: String) -> Flow {
        self.pending = Some(action);
        self.composer.set_text(initial);
        self.screen = Screen::Prompt;
        Flow::Stay
    }

    fn update_prompt(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Detail;
                self.pending = None;
                self.composer.clear();
                Flow::Stay
            }
            KeyCode::Enter => self.confirm_prompt(),
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

    fn confirm_prompt(&mut self) -> Flow {
        let Some(action) = self.pending else {
            self.screen = Screen::Detail;
            return Flow::Stay;
        };
        let Some(row) = self.selected.clone() else {
            self.screen = Screen::List;
            return Flow::Stay;
        };
        let text = self.composer.text().trim().to_string();
        // Reject/Archive need a reason; Edit needs content.
        if text.is_empty() {
            self.set_status("Enter a value, or press Esc to cancel.");
            return Flow::Stay;
        }
        let mutation = match action {
            PendingAction::Reject => Mutation::Reject {
                id: row.id,
                reason: text,
            },
            PendingAction::Archive => Mutation::Archive {
                id: row.id,
                reason: text,
            },
            PendingAction::Edit => Mutation::Edit {
                id: row.id,
                content: text,
            },
        };
        self.pending = None;
        self.composer.clear();
        Flow::Mutate(mutation)
    }
}

pub(crate) fn status_label(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Approved => "approved",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Archived => "archived",
    }
}

pub(crate) fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}
