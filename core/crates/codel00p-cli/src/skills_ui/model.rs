//! Pure state and transitions for the `codel00p skills` review dialog.
//!
//! State changes happen only in [`SkillsModel::update`], which maps a key event
//! to a [`Flow`]; the driver in the parent module loads skills/candidates,
//! opens a detail view, and performs review effects ([`Flow::Mutate`]). The model
//! never touches the skill store, so it is fully testable by feeding synthetic
//! key events.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::picker::{Picker, PickerItem, PickerOutcome};

/// Whether a row is an active skill or a candidate awaiting review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkillKind {
    Active,
    Candidate,
}

/// A skill (active or candidate) projected for the list picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkillRow {
    pub(crate) name: String,
    pub(crate) kind: SkillKind,
    /// Source label (`user`/`project`/`bundled`).
    pub(crate) source: String,
    /// Provenance (`agent`/`user`), when recorded.
    pub(crate) created_by: Option<String>,
    /// Usage count for active skills (always 0 for candidates).
    pub(crate) usage: u64,
    pub(crate) description: String,
    /// The SKILL.md instructions body, shown on the detail screen.
    pub(crate) body: String,
    pub(crate) version: Option<String>,
    pub(crate) triggers: Vec<String>,
    pub(crate) path: String,
    /// The skills-root directory this skill lives under, so the driver can
    /// approve/reject/archive without re-deriving it.
    pub(crate) root: PathBuf,
}

impl PickerItem for SkillRow {
    fn label(&self) -> String {
        self.name.clone()
    }
    fn detail(&self) -> Option<String> {
        let usage = match self.kind {
            SkillKind::Candidate => "candidate".to_string(),
            SkillKind::Active if self.usage == 0 => "unused".to_string(),
            SkillKind::Active => format!("used {}x", self.usage),
        };
        Some(format!("{} · {usage}", self.source))
    }
}

/// Which skills the list is filtered to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filter {
    Active,
    Candidates,
    All,
}

impl Filter {
    pub(crate) const ORDER: [Filter; 3] = [Filter::Active, Filter::Candidates, Filter::All];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Filter::Active => "Active",
            Filter::Candidates => "Candidates",
            Filter::All => "All",
        }
    }

    /// Whether a row of the given kind is visible under this filter.
    pub(crate) fn shows(self, kind: SkillKind) -> bool {
        match self {
            Filter::Active => kind == SkillKind::Active,
            Filter::Candidates => kind == SkillKind::Candidate,
            Filter::All => true,
        }
    }

    fn next(self) -> Filter {
        let index = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        Self::ORDER[(index + 1) % Self::ORDER.len()]
    }

    fn prev(self) -> Filter {
        let index = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        Self::ORDER[(index + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// A review effect for the driver to apply against the skill store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mutation {
    /// Approve a candidate (it becomes active).
    Approve { name: String, root: PathBuf },
    /// Reject a candidate (archived, reversible).
    Reject { name: String, root: PathBuf },
    /// Disable an active skill (archived, reversible).
    Disable { name: String, root: PathBuf },
}

/// The screen currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Detail,
}

/// What the driver should do after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Stay,
    OpenDetail,
    Mutate(Mutation),
    Quit,
}

pub(crate) struct SkillsModel {
    pub(crate) filter: Filter,
    pub(crate) picker: Picker<SkillRow>,
    pub(crate) screen: Screen,
    pub(crate) selected: Option<SkillRow>,
    pub(crate) scroll: usize,
    pub(crate) status: Option<String>,
    /// All rows across kinds; the picker shows the subset matching `filter`.
    all_rows: Vec<SkillRow>,
}

impl SkillsModel {
    pub(crate) fn new() -> Self {
        SkillsModel {
            filter: Filter::Active,
            picker: Picker::new(Vec::new()),
            screen: Screen::List,
            selected: None,
            scroll: 0,
            status: None,
            all_rows: Vec::new(),
        }
    }

    /// Replaces the full row set (called by the driver on load/reload) and
    /// re-applies the current filter to the picker.
    pub(crate) fn set_rows(&mut self, rows: Vec<SkillRow>) {
        self.all_rows = rows;
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let visible: Vec<SkillRow> = self
            .all_rows
            .iter()
            .filter(|row| self.filter.shows(row.kind))
            .cloned()
            .collect();
        self.picker.set_items(visible);
    }

    /// Sets a transient status line (e.g. after a mutation).
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
        }
    }

    fn update_list(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Tab => {
                self.filter = self.filter.next();
                self.apply_filter();
                Flow::Stay
            }
            KeyCode::BackTab => {
                self.filter = self.filter.prev();
                self.apply_filter();
                Flow::Stay
            }
            KeyCode::Char('a') => self.act(SkillKind::Candidate, |row| Mutation::Approve {
                name: row.name.clone(),
                root: row.root.clone(),
            }),
            KeyCode::Char('r') => self.act(SkillKind::Candidate, |row| Mutation::Reject {
                name: row.name.clone(),
                root: row.root.clone(),
            }),
            KeyCode::Char('d') => self.act(SkillKind::Active, |row| Mutation::Disable {
                name: row.name.clone(),
                root: row.root.clone(),
            }),
            _ => match self.picker.on_key(key) {
                PickerOutcome::Selected => match self.picker.selected_item().cloned() {
                    Some(row) => {
                        self.selected = Some(row);
                        self.scroll = 0;
                        self.screen = Screen::Detail;
                        Flow::OpenDetail
                    }
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
                self.selected = None;
                self.scroll = 0;
                Flow::Stay
            }
            KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Flow::Stay
            }
            KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Flow::Stay
            }
            KeyCode::Char('a') if row.kind == SkillKind::Candidate => {
                Flow::Mutate(Mutation::Approve {
                    name: row.name,
                    root: row.root,
                })
            }
            KeyCode::Char('r') if row.kind == SkillKind::Candidate => {
                Flow::Mutate(Mutation::Reject {
                    name: row.name,
                    root: row.root,
                })
            }
            KeyCode::Char('d') if row.kind == SkillKind::Active => {
                Flow::Mutate(Mutation::Disable {
                    name: row.name,
                    root: row.root,
                })
            }
            _ => Flow::Stay,
        }
    }

    /// Maps an action key to a mutation for the highlighted row when it matches
    /// `kind`; otherwise hints why the key did nothing.
    fn act(&mut self, kind: SkillKind, mutation: impl Fn(&SkillRow) -> Mutation) -> Flow {
        match self.picker.selected_item() {
            Some(row) if row.kind == kind => Flow::Mutate(mutation(row)),
            Some(_) => {
                self.set_status(match kind {
                    SkillKind::Candidate => "Approve/reject applies to candidates only.",
                    SkillKind::Active => "Disable applies to active skills only.",
                });
                Flow::Stay
            }
            None => Flow::Stay,
        }
    }
}
