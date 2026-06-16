//! Overlay panels drawn over the chat: pickers, the entity browser, the permission
//! modal, and help. All state here is pure and terminal-independent.

use codel00p_harness::PermissionRequest;
use codel00p_protocol::{Agent, McpServer, MemoryEntry, OrgMember, OrgRef, OrgRole, Project};
use crossterm::event::KeyEvent;

use super::picker::{Picker, PickerItem, PickerOutcome};

/// A selectable model, either a known provider/model pair or a free-text entry that
/// lets any model id through (matching the CLI's unchecked `/model <id>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelChoice {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) note: Option<String>,
}

impl PickerItem for ModelChoice {
    fn label(&self) -> String {
        format!("{} · {}", self.provider, self.model)
    }
    fn detail(&self) -> Option<String> {
        self.note.clone()
    }
}

/// The model picker overlay: a `Picker<ModelChoice>` plus a transient status line
/// that reports the live `list_models` fetch (loading / fell back to the catalog).
#[derive(Clone, Debug)]
pub(crate) struct ModelPicker {
    pub(crate) picker: Picker<ModelChoice>,
    pub(crate) status: Option<String>,
}

impl ModelPicker {
    pub(crate) fn new(choices: Vec<ModelChoice>, status: Option<String>) -> Self {
        Self {
            picker: Picker::new(choices),
            status,
        }
    }

    /// Replaces the catalog rows with the live `list_models` result and clears the
    /// loading status. Preserves the picker's current filter text.
    pub(crate) fn set_choices(&mut self, choices: Vec<ModelChoice>, status: Option<String>) {
        self.picker.set_items(choices);
        self.status = status;
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.picker.on_key(key)
    }

    pub(crate) fn selected_item(&self) -> Option<&ModelChoice> {
        self.picker.selected_item()
    }

    /// The free-text model id typed into the filter, used when no catalog row is
    /// highlighted (Enter on an empty filter result), mirroring `/model <id>`.
    pub(crate) fn free_text(&self) -> &str {
        self.picker.query()
    }
}

/// A prior conversation shown in the session switcher overlay. Read-only — selecting
/// one replays it and resets the live conversation to that session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionSummary {
    pub(crate) session_id: String,
    pub(crate) source: String,
    pub(crate) message_count: usize,
}

impl PickerItem for SessionSummary {
    fn label(&self) -> String {
        self.session_id.clone()
    }
    fn detail(&self) -> Option<String> {
        Some(format!(
            "{} · {} message(s)",
            self.source, self.message_count
        ))
    }
}

impl PickerItem for Project {
    fn label(&self) -> String {
        self.name().to_string()
    }
    fn detail(&self) -> Option<String> {
        self.repository_url().map(|url| url.to_string())
    }
}

impl PickerItem for Agent {
    fn label(&self) -> String {
        self.name().to_string()
    }
    fn detail(&self) -> Option<String> {
        Some(format!("{} · {}", self.provider(), self.model()))
    }
}

impl PickerItem for McpServer {
    fn label(&self) -> String {
        self.name().to_string()
    }
    fn detail(&self) -> Option<String> {
        let transport = match self.transport() {
            codel00p_protocol::McpTransport::Stdio => "stdio",
            codel00p_protocol::McpTransport::Http => "http",
        };
        let target = self.command().or(self.url()).unwrap_or("");
        let state = if self.enabled() {
            "enabled"
        } else {
            "disabled"
        };
        Some(format!("{transport} · {state} · {target}"))
    }
}

impl PickerItem for MemoryEntry {
    fn label(&self) -> String {
        self.content().to_string()
    }
    fn detail(&self) -> Option<String> {
        Some(format!("{:?} · {:?}", self.kind(), self.status()))
    }
}

impl PickerItem for OrgMember {
    fn label(&self) -> String {
        self.name()
            .or(self.email())
            .unwrap_or_else(|| self.user_id())
            .to_string()
    }
    fn detail(&self) -> Option<String> {
        let role = match self.role() {
            OrgRole::Admin => "admin",
            OrgRole::Member => "member",
        };
        match self.email() {
            Some(email) if email != self.label() => Some(format!("{role} · {email}")),
            _ => Some(role.to_string()),
        }
    }
}

impl PickerItem for OrgRef {
    fn label(&self) -> String {
        self.name().to_string()
    }
    fn detail(&self) -> Option<String> {
        match self.slug() {
            Some(slug) => Some(format!("{} · {slug}", self.id())),
            None => Some(self.id().to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntityTab {
    Projects,
    Agents,
    Mcp,
    Memory,
    Users,
    Org,
}

impl EntityTab {
    pub(crate) const ORDER: [EntityTab; 6] = [
        EntityTab::Projects,
        EntityTab::Agents,
        EntityTab::Mcp,
        EntityTab::Memory,
        EntityTab::Users,
        EntityTab::Org,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            EntityTab::Projects => "Projects",
            EntityTab::Agents => "Agents",
            EntityTab::Mcp => "MCP",
            EntityTab::Memory => "Memory",
            EntityTab::Users => "Users",
            EntityTab::Org => "Org",
        }
    }

    pub(crate) fn next(self) -> EntityTab {
        let position = Self::ORDER.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ORDER[(position + 1) % Self::ORDER.len()]
    }

    pub(crate) fn prev(self) -> EntityTab {
        let position = Self::ORDER.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ORDER[(position + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// The org entity browser: a tab strip over project-scoped entity lists plus an
/// organization picker that can re-authenticate into another Clerk org.
#[derive(Clone, Debug)]
pub(crate) struct EntityBrowser {
    pub(crate) tab: EntityTab,
    pub(crate) projects: Picker<Project>,
    pub(crate) agents: Picker<Agent>,
    pub(crate) mcp: Picker<McpServer>,
    pub(crate) memory: Picker<MemoryEntry>,
    pub(crate) users: Picker<OrgMember>,
    pub(crate) orgs: Picker<OrgRef>,
    /// The project whose agents/MCP/memory are currently shown.
    pub(crate) selected_project: Option<Project>,
    pub(crate) status: Option<String>,
}

impl EntityBrowser {
    pub(crate) fn new(tab: EntityTab) -> Self {
        Self {
            tab,
            projects: Picker::new(Vec::new()),
            agents: Picker::new(Vec::new()),
            mcp: Picker::new(Vec::new()),
            memory: Picker::new(Vec::new()),
            users: Picker::new(Vec::new()),
            orgs: Picker::new(Vec::new()),
            selected_project: None,
            status: Some("Loading…".to_string()),
        }
    }
}

/// The session switcher: a read-only list of prior conversations plus a status line
/// (loading / error). Selecting a row resumes that session in place.
#[derive(Clone, Debug)]
pub(crate) struct SessionSwitcher {
    pub(crate) sessions: Picker<SessionSummary>,
    pub(crate) status: Option<String>,
}

impl SessionSwitcher {
    pub(crate) fn new() -> Self {
        Self {
            sessions: Picker::new(Vec::new()),
            status: Some("Loading…".to_string()),
        }
    }

    pub(crate) fn set_sessions(&mut self, sessions: Vec<SessionSummary>, status: Option<String>) {
        self.sessions.set_items(sessions);
        self.status = status;
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.sessions.on_key(key)
    }

    pub(crate) fn selected_item(&self) -> Option<&SessionSummary> {
        self.sessions.selected_item()
    }
}

/// An action reachable from the command palette. Each maps to an existing handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandAction {
    Model,
    Sessions,
    NewConversation,
    Browse,
    Users,
    SwitchOrg,
    History,
    Tools,
    Help,
    Quit,
}

/// One row in the command palette: a label, a short hint, and the action it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandItem {
    pub(crate) label: String,
    pub(crate) hint: &'static str,
    pub(crate) action: CommandAction,
}

impl PickerItem for CommandItem {
    fn label(&self) -> String {
        self.label.clone()
    }
    fn detail(&self) -> Option<String> {
        Some(self.hint.to_string())
    }
}

/// The VSCode-style command palette: a fuzzy-filterable list of every CLI action,
/// so users do not have to remember the individual F-key surfaces.
#[derive(Clone, Debug)]
pub(crate) struct CommandPalette {
    pub(crate) picker: Picker<CommandItem>,
}

impl CommandPalette {
    pub(crate) fn new() -> Self {
        Self {
            picker: Picker::new(command_items()),
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.picker.on_key(key)
    }

    pub(crate) fn selected_item(&self) -> Option<&CommandItem> {
        self.picker.selected_item()
    }
}

/// The full command catalog, in display order.
pub(crate) fn command_items() -> Vec<CommandItem> {
    use CommandAction::*;
    [
        ("Switch model", "pick a provider / model", Model),
        ("Switch session", "resume a prior conversation", Sessions),
        ("New conversation", "start a fresh chat", NewConversation),
        (
            "Browse organization",
            "projects · agents · MCP · memory",
            Browse,
        ),
        ("Browse users", "organization members", Users),
        ("Switch organization", "re-auth into another org", SwitchOrg),
        ("Show history", "this conversation's messages", History),
        ("Show tools", "enabled tool sets", Tools),
        ("Help", "keys and commands", Help),
        ("Quit", "exit codel00p", Quit),
    ]
    .into_iter()
    .map(|(label, hint, action)| CommandItem {
        label: label.to_string(),
        hint,
        action,
    })
    .collect()
}

// There is only ever one live `Overlay` (the open panel), so the size spread
// between variants is irrelevant; boxing would just add indirection for no gain.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Overlay {
    None,
    Help,
    Permission(PermissionRequest),
    Model(ModelPicker),
    Sessions(SessionSwitcher),
    Entities(EntityBrowser),
    Command(CommandPalette),
}

impl Overlay {
    pub(crate) fn is_open(&self) -> bool {
        !matches!(self, Overlay::None)
    }
}
