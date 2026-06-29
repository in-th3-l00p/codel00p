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

/// A curator finding attached to a near-duplicate approved memory: the survivor
/// it duplicates and the shingle similarity (0..=100). Set by the driver from
/// `plan_consolidations` when the curator is enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NearDuplicate {
    pub(crate) survivor: String,
    pub(crate) similarity: u8,
}

/// A memory record projected for the list picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemoryRow {
    pub(crate) id: String,
    pub(crate) status: MemoryStatus,
    pub(crate) kind: MemoryKind,
    pub(crate) content: String,
    pub(crate) tags: Vec<String>,
    /// Curator finding: when set, this approved memory is a near-duplicate of
    /// `survivor` and `c` (on the detail screen) archives it (keeping the survivor).
    pub(crate) near_duplicate_of: Option<NearDuplicate>,
}

impl PickerItem for MemoryRow {
    fn label(&self) -> String {
        self.content.clone()
    }
    fn detail(&self) -> Option<String> {
        let mut detail = format!("{} · {}", status_label(self.status), kind_label(self.kind));
        if let Some(dup) = &self.near_duplicate_of {
            detail.push_str(&format!(
                " · ~dup of {} ({}%)",
                dup.survivor, dup.similarity
            ));
        }
        Some(detail)
    }
}

/// One row of a record's audit trail, shown on the detail screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuditRow {
    pub(crate) sequence: u64,
    pub(crate) action: String,
    pub(crate) actor: String,
    pub(crate) reason: Option<String>,
    /// The content this event replaced, if any. Only rows with a value here are
    /// restorable (`u` on the detail screen).
    pub(crate) previous_content: Option<String>,
}

/// A restorable audit entry, projected for the restore picker. Built purely from
/// the audit rows that carry a `previous_content`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoreRow {
    pub(crate) sequence: u64,
    pub(crate) action: String,
    pub(crate) previous_content: String,
}

impl PickerItem for RestoreRow {
    fn label(&self) -> String {
        self.previous_content.clone()
    }
    fn detail(&self) -> Option<String> {
        Some(format!("#{} · {}", self.sequence, self.action))
    }
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
    Merge { source: String, target: String },
    Restore { id: String, sequence: u64 },
}

/// The screen currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Detail,
    Prompt,
    /// Pick a target active record to merge the current record into.
    SelectMerge,
    /// Pick a restorable audit entry to roll the content back to.
    SelectRestore,
}

/// What the driver should do after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Stay,
    Reload,
    OpenDetail(String),
    /// Ask the driver to load the active records (other than this source) and
    /// hand them back via [`MemoryModel::show_merge_picker`].
    LoadMergeTargets(String),
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
    /// When `true`, the `?` help overlay is shown and swallows all keys.
    pub(crate) show_help: bool,
    /// Target records for the merge picker, loaded by the driver on demand.
    pub(crate) merge_targets: Picker<MemoryRow>,
    /// Restorable audit entries for the restore picker, built from the audit.
    pub(crate) restore_picker: Picker<RestoreRow>,
    /// Whether the opt-in curator is enabled (controls the `c` consolidate hint).
    pub(crate) curator_enabled: bool,
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
            show_help: false,
            merge_targets: Picker::new(Vec::new()),
            restore_picker: Picker::new(Vec::new()),
            curator_enabled: false,
        }
    }

    /// Records whether the opt-in curator is enabled (driver-provided), so the
    /// `c` consolidate action can give an accurate hint when it does nothing.
    pub(crate) fn set_curator_enabled(&mut self, enabled: bool) {
        self.curator_enabled = enabled;
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

    /// Opens the merge target picker with the driver-loaded candidate records.
    /// If none are available, stays on the detail screen with a status note.
    pub(crate) fn show_merge_picker(&mut self, targets: Vec<MemoryRow>) {
        if targets.is_empty() {
            self.set_status("No other active records to merge into.");
            self.screen = Screen::Detail;
            return;
        }
        self.merge_targets = Picker::new(targets);
        self.screen = Screen::SelectMerge;
    }

    /// The restorable audit entries: those carrying a `previous_content`, newest
    /// first so the most recent prior content is offered at the top.
    fn restorable_rows(&self) -> Vec<RestoreRow> {
        let mut rows: Vec<RestoreRow> = self
            .detail_audit
            .iter()
            .filter_map(|event| {
                event.previous_content.as_ref().map(|content| RestoreRow {
                    sequence: event.sequence,
                    action: event.action.clone(),
                    previous_content: content.clone(),
                })
            })
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.sequence));
        rows
    }

    /// Sets a transient status line (e.g. after a successful mutation).
    pub(crate) fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
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
        // `?` toggles help — but not on the text-entry prompt, where it is a
        // legitimate character in a reason/edit (the prompt already serves as the
        // confirm step for the destructive reject/archive actions).
        if key.code == KeyCode::Char('?') && self.screen != Screen::Prompt {
            self.show_help = true;
            return Flow::Stay;
        }
        match self.screen {
            Screen::List => self.update_list(key),
            Screen::Detail => self.update_detail(key),
            Screen::Prompt => self.update_prompt(key),
            Screen::SelectMerge => self.update_select_merge(key),
            Screen::SelectRestore => self.update_select_restore(key),
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
            KeyCode::Char('m') => {
                self.status = None;
                Flow::LoadMergeTargets(row.id)
            }
            KeyCode::Char('c') => self.consolidate(&row),
            KeyCode::Char('u') => self.open_restore_picker(),
            _ => Flow::Stay,
        }
    }

    /// `c` (consolidate): if this approved memory is a curator-flagged
    /// near-duplicate, archive it (reversible) with an auto reason naming the
    /// survivor; otherwise explain why nothing happened. Archive is reversible via
    /// the audit trail, so no extra confirmation step is needed.
    fn consolidate(&mut self, row: &MemoryRow) -> Flow {
        match &row.near_duplicate_of {
            Some(dup) => {
                self.set_status(format!(
                    "Archiving {} as a near-duplicate of {} ({}% similar).",
                    row.id, dup.survivor, dup.similarity
                ));
                Flow::Mutate(Mutation::Archive {
                    id: row.id.clone(),
                    reason: format!(
                        "curator: near-duplicate of {} ({}% similar)",
                        dup.survivor, dup.similarity
                    ),
                })
            }
            None if !self.curator_enabled => {
                self.set_status(
                    "Enable agent.behavior.curator to surface near-duplicate memories (config set agent.behavior.curator true).",
                );
                Flow::Stay
            }
            None => {
                self.set_status(format!(
                    "{} is not a near-duplicate of another approved memory.",
                    row.id
                ));
                Flow::Stay
            }
        }
    }

    /// Builds the restore picker from the loaded audit. With no restorable entry
    /// it stays on detail and notes why.
    fn open_restore_picker(&mut self) -> Flow {
        let rows = self.restorable_rows();
        if rows.is_empty() {
            self.set_status("No prior content to restore for this record.");
            return Flow::Stay;
        }
        self.restore_picker = Picker::new(rows);
        self.screen = Screen::SelectRestore;
        Flow::Stay
    }

    fn update_select_merge(&mut self, key: KeyEvent) -> Flow {
        let Some(source) = self.selected.clone() else {
            self.screen = Screen::List;
            return Flow::Stay;
        };
        match self.merge_targets.on_key(key) {
            PickerOutcome::Selected => match self.merge_targets.selected_item().cloned() {
                Some(target) => Flow::Mutate(Mutation::Merge {
                    source: source.id,
                    target: target.id,
                }),
                None => Flow::Stay,
            },
            PickerOutcome::Cancelled => {
                self.screen = Screen::Detail;
                Flow::Stay
            }
            PickerOutcome::Pending => Flow::Stay,
        }
    }

    fn update_select_restore(&mut self, key: KeyEvent) -> Flow {
        let Some(row) = self.selected.clone() else {
            self.screen = Screen::List;
            return Flow::Stay;
        };
        match self.restore_picker.on_key(key) {
            PickerOutcome::Selected => match self.restore_picker.selected_item() {
                Some(entry) => Flow::Mutate(Mutation::Restore {
                    id: row.id,
                    sequence: entry.sequence,
                }),
                None => Flow::Stay,
            },
            PickerOutcome::Cancelled => {
                self.screen = Screen::Detail;
                Flow::Stay
            }
            PickerOutcome::Pending => Flow::Stay,
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
