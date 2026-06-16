//! Pure state and transitions for the `codel00p cloud` dialog.
//!
//! Every state change happens in [`CloudModel::update`], which maps a key event
//! to a [`Flow`]; all IO (terminal setup, cloud fetches, push/pull) lives in the
//! parent module's driver. The model is store-free — it holds only projected rows
//! and preformatted lines — so it is testable by feeding synthetic key events.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::picker::{Picker, PickerItem, PickerOutcome};

/// A project projected for the list picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) detail: Option<String>,
}

impl PickerItem for ProjectRow {
    fn label(&self) -> String {
        self.name.clone()
    }
    fn detail(&self) -> Option<String> {
        self.detail.clone()
    }
}

/// A project entity (agent / MCP server / memory) projected for a picker. The
/// entity-specific data is flattened into `label` + `detail` so the detail screen
/// renders every tab the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntityRow {
    pub(crate) label: String,
    pub(crate) detail: Option<String>,
}

impl PickerItem for EntityRow {
    fn label(&self) -> String {
        self.label.clone()
    }
    fn detail(&self) -> Option<String> {
        self.detail.clone()
    }
}

/// The screen currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    /// Signed-in landing: viewer status + the project list, with push/pull actions.
    Status,
    /// A single project's agents / MCP servers / memory, in tabs.
    Detail,
    /// Not signed in: a "run `codel00p auth login`" message.
    Unauthenticated,
}

/// The detail screen's sub-tab over a project's entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DetailTab {
    Agents,
    Mcp,
    Memory,
}

impl DetailTab {
    pub(crate) const ORDER: [DetailTab; 3] = [DetailTab::Agents, DetailTab::Mcp, DetailTab::Memory];

    pub(crate) fn title(self) -> &'static str {
        match self {
            DetailTab::Agents => "Agents",
            DetailTab::Mcp => "MCP",
            DetailTab::Memory => "Memory",
        }
    }

    pub(crate) fn next(self) -> DetailTab {
        let position = Self::ORDER.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ORDER[(position + 1) % Self::ORDER.len()]
    }

    pub(crate) fn prev(self) -> DetailTab {
        let position = Self::ORDER.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ORDER[(position + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// What the driver should do after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Stay,
    /// Fetch the project's entities and open the detail screen.
    OpenProject(String),
    /// Push local memory to the active cloud project.
    Push,
    /// Pull approved cloud memory into the local store.
    Pull,
    Quit,
}

pub(crate) struct CloudModel {
    pub(crate) screen: Screen,
    /// Preformatted viewer/status lines (signed-in user + active org).
    pub(crate) viewer_lines: Vec<String>,
    pub(crate) projects: Picker<ProjectRow>,
    /// The project whose entities the detail screen shows.
    pub(crate) selected_project: Option<ProjectRow>,
    pub(crate) tab: DetailTab,
    pub(crate) agents: Picker<EntityRow>,
    pub(crate) mcp: Picker<EntityRow>,
    pub(crate) memory: Picker<EntityRow>,
    /// A transient action/result line shown under the project list.
    pub(crate) status: Option<String>,
}

impl CloudModel {
    /// A signed-in model showing the viewer summary and project list.
    pub(crate) fn signed_in(viewer_lines: Vec<String>, projects: Vec<ProjectRow>) -> Self {
        CloudModel {
            screen: Screen::Status,
            viewer_lines,
            projects: Picker::new(projects),
            selected_project: None,
            tab: DetailTab::Agents,
            agents: Picker::new(Vec::new()),
            mcp: Picker::new(Vec::new()),
            memory: Picker::new(Vec::new()),
            status: None,
        }
    }

    /// A not-signed-in model showing `message` and a sign-in hint.
    pub(crate) fn unauthenticated(message: String) -> Self {
        CloudModel {
            screen: Screen::Unauthenticated,
            viewer_lines: vec![message],
            projects: Picker::new(Vec::new()),
            selected_project: None,
            tab: DetailTab::Agents,
            agents: Picker::new(Vec::new()),
            mcp: Picker::new(Vec::new()),
            memory: Picker::new(Vec::new()),
            status: None,
        }
    }

    /// Opens the detail screen for `project` with its (driver-loaded) entities.
    pub(crate) fn show_detail(
        &mut self,
        project: ProjectRow,
        agents: Vec<EntityRow>,
        mcp: Vec<EntityRow>,
        memory: Vec<EntityRow>,
    ) {
        self.selected_project = Some(project);
        self.agents.set_items(agents);
        self.mcp.set_items(mcp);
        self.memory.set_items(memory);
        self.tab = DetailTab::Agents;
        self.screen = Screen::Detail;
    }

    /// Records the result of an action (push/pull) on the status line.
    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = Some(status.into());
    }

    /// The picker backing the active detail tab.
    pub(crate) fn active_tab_picker(&self) -> &Picker<EntityRow> {
        match self.tab {
            DetailTab::Agents => &self.agents,
            DetailTab::Mcp => &self.mcp,
            DetailTab::Memory => &self.memory,
        }
    }

    pub(crate) fn update(&mut self, key: KeyEvent) -> Flow {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Flow::Quit;
        }
        match self.screen {
            Screen::Status => self.update_status(key),
            Screen::Detail => self.update_detail(key),
            Screen::Unauthenticated => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => Flow::Quit,
                _ => Flow::Stay,
            },
        }
    }

    fn update_status(&mut self, key: KeyEvent) -> Flow {
        // Action keys take priority over the picker's text filter so push/pull
        // stay reachable from the list.
        match key.code {
            KeyCode::Char('p') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Flow::Push;
            }
            KeyCode::Char('l') => return Flow::Pull,
            _ => {}
        }
        match self.projects.on_key(key) {
            PickerOutcome::Selected => match self.projects.selected_item().cloned() {
                Some(row) => Flow::OpenProject(row.id),
                None => Flow::Stay,
            },
            PickerOutcome::Cancelled => Flow::Quit,
            PickerOutcome::Pending => Flow::Stay,
        }
    }

    fn update_detail(&mut self, key: KeyEvent) -> Flow {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Status;
                Flow::Stay
            }
            KeyCode::Tab | KeyCode::Right => {
                self.tab = self.tab.next();
                Flow::Stay
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.tab = self.tab.prev();
                Flow::Stay
            }
            _ => {
                // Navigation (and filtering) flow to the active tab's picker;
                // Esc is handled above so the picker never sees it.
                let picker = match self.tab {
                    DetailTab::Agents => &mut self.agents,
                    DetailTab::Mcp => &mut self.mcp,
                    DetailTab::Memory => &mut self.memory,
                };
                picker.on_key(key);
                Flow::Stay
            }
        }
    }
}
