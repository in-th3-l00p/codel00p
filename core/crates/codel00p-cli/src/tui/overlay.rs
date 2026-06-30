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

/// A prior conversation shown in the session switcher overlay. Selecting one
/// replays it; `e` edits its name + description, `d` deletes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionSummary {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) source: String,
    pub(crate) message_count: usize,
}

impl PickerItem for SessionSummary {
    fn label(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.session_id.clone())
    }
    fn detail(&self) -> Option<String> {
        let meta = if self.title.is_some() {
            format!(
                "{} · {} · {} message(s)",
                self.session_id, self.source, self.message_count
            )
        } else {
            format!("{} · {} message(s)", self.source, self.message_count)
        };
        match &self.description {
            Some(description) if !description.is_empty() => Some(format!("{meta} — {description}")),
            _ => Some(meta),
        }
    }
}

/// A row in the conversations overlay: the always-first "new conversation" action,
/// then one row per prior conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConversationRow {
    /// Start a fresh conversation. Always the first row.
    New,
    /// Resume / edit / delete an existing conversation.
    Session(SessionSummary),
}

impl PickerItem for ConversationRow {
    fn label(&self) -> String {
        match self {
            ConversationRow::New => "＋ New conversation".to_string(),
            ConversationRow::Session(summary) => summary.label(),
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            ConversationRow::New => Some("start a fresh chat".to_string()),
            ConversationRow::Session(summary) => summary.detail(),
        }
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

/// An in-progress rename inside the session switcher: the id of the session being
/// renamed and the editable title buffer (a single line, cursor parked at the end).
/// Which field the inline conversation editor is focused on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionEditField {
    Name,
    Description,
}

/// State for editing a conversation's name + description inline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionEdit {
    pub(crate) session_id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) field: SessionEditField,
}

impl SessionEdit {
    /// Moves focus between the name and description fields (Tab).
    pub(crate) fn toggle_field(&mut self) {
        self.field = match self.field {
            SessionEditField::Name => SessionEditField::Description,
            SessionEditField::Description => SessionEditField::Name,
        };
    }

    /// The buffer for the currently focused field.
    pub(crate) fn active_buffer_mut(&mut self) -> &mut String {
        match self.field {
            SessionEditField::Name => &mut self.name,
            SessionEditField::Description => &mut self.description,
        }
    }
}

/// State for the delete-confirmation prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionDelete {
    pub(crate) session_id: String,
    /// A display label (title or id) for the confirm prompt.
    pub(crate) label: String,
}

/// The conversations overlay: a "＋ New conversation" row followed by the prior
/// conversations, plus a status line. Selecting a session resumes it in place; `e`
/// opens an inline name+description editor and `d` a delete confirmation.
#[derive(Clone, Debug)]
pub(crate) struct SessionSwitcher {
    pub(crate) rows: Picker<ConversationRow>,
    pub(crate) status: Option<String>,
    /// `Some` while editing a conversation's name + description inline.
    pub(crate) edit: Option<SessionEdit>,
    /// `Some` while confirming a conversation deletion.
    pub(crate) confirm_delete: Option<SessionDelete>,
}

impl SessionSwitcher {
    pub(crate) fn new() -> Self {
        Self {
            rows: Picker::new(vec![ConversationRow::New]),
            status: Some("Loading…".to_string()),
            edit: None,
            confirm_delete: None,
        }
    }

    /// Rebuilds the row list as `[New, …sessions]` and sets the status line.
    pub(crate) fn set_sessions(&mut self, sessions: Vec<SessionSummary>, status: Option<String>) {
        let rows = std::iter::once(ConversationRow::New)
            .chain(sessions.into_iter().map(ConversationRow::Session))
            .collect();
        self.rows.set_items(rows);
        self.status = status;
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.rows.on_key(key)
    }

    pub(crate) fn selected_row(&self) -> Option<&ConversationRow> {
        self.rows.selected_item()
    }

    /// The highlighted session, or `None` when the "New conversation" row (or
    /// nothing) is highlighted.
    pub(crate) fn selected_session(&self) -> Option<&SessionSummary> {
        match self.rows.selected_item() {
            Some(ConversationRow::Session(summary)) => Some(summary),
            _ => None,
        }
    }

    /// Enters edit mode for the highlighted session, seeding the fields with its
    /// current name (falling back to the id) and description. No-op on the New row.
    pub(crate) fn begin_edit(&mut self) {
        if let Some(session) = self.selected_session() {
            self.edit = Some(SessionEdit {
                session_id: session.session_id.clone(),
                name: session
                    .title
                    .clone()
                    .unwrap_or_else(|| session.session_id.clone()),
                description: session.description.clone().unwrap_or_default(),
                field: SessionEditField::Name,
            });
        }
    }

    /// Opens the delete confirmation for the highlighted session. No-op on New.
    pub(crate) fn begin_delete(&mut self) {
        if let Some(session) = self.selected_session() {
            self.confirm_delete = Some(SessionDelete {
                session_id: session.session_id.clone(),
                label: session
                    .title
                    .clone()
                    .unwrap_or_else(|| session.session_id.clone()),
            });
        }
    }
}

/// A selectable local agent (multi-agent personas, #13) shown in the agent
/// switcher overlay. `name` is the registry name (`default` for the base home);
/// `active` marks the agent the running TUI is currently pointed at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentChoice {
    /// Registry name, or `default` for the base home.
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    /// `true` for the agent the TUI is currently using (its memory + sessions).
    pub(crate) active: bool,
}

impl PickerItem for AgentChoice {
    fn label(&self) -> String {
        if self.active {
            format!("{} ✓", self.name)
        } else {
            self.name.clone()
        }
    }
    fn detail(&self) -> Option<String> {
        match (&self.description, self.active) {
            (Some(description), true) => Some(format!("active · {description}")),
            (Some(description), false) => Some(description.clone()),
            (None, true) => Some("active".to_string()),
            (None, false) => None,
        }
    }
}

/// A row in the agent overlay: the always-first "new agent" action, then one row
/// per local agent (default + every `<base>/agents/<name>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentRow {
    /// Define a new local agent. Always the first row.
    New,
    /// Use / edit / delete an existing agent.
    Agent(AgentChoice),
}

impl PickerItem for AgentRow {
    fn label(&self) -> String {
        match self {
            AgentRow::New => "＋ New agent".to_string(),
            AgentRow::Agent(choice) => choice.label(),
        }
    }
    fn detail(&self) -> Option<String> {
        match self {
            AgentRow::New => Some("define a new local agent".to_string()),
            AgentRow::Agent(choice) => choice.detail(),
        }
    }
}

/// State for the delete-confirmation prompt in the agent overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentDelete {
    pub(crate) name: String,
}

/// The agent overlay: a "＋ New agent" row followed by the local agents (default +
/// every `<base>/agents/<name>`), plus a status line. Selecting an agent performs
/// a LIVE switch — re-pointing the running TUI at that agent's home so subsequent
/// turns use its memory and sessions. `d` deletes a non-active, non-default agent.
#[derive(Clone, Debug)]
pub(crate) struct AgentSwitcher {
    pub(crate) rows: Picker<AgentRow>,
    pub(crate) status: Option<String>,
    /// `Some` while confirming an agent deletion.
    pub(crate) confirm_delete: Option<AgentDelete>,
}

impl AgentSwitcher {
    pub(crate) fn new() -> Self {
        Self {
            rows: Picker::new(vec![AgentRow::New]),
            status: Some("Loading…".to_string()),
            confirm_delete: None,
        }
    }

    /// Rebuilds the row list as `[New, …agents]` and sets the status line.
    pub(crate) fn set_agents(&mut self, agents: Vec<AgentChoice>, status: Option<String>) {
        let rows = std::iter::once(AgentRow::New)
            .chain(agents.into_iter().map(AgentRow::Agent))
            .collect();
        self.rows.set_items(rows);
        self.status = status;
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.rows.on_key(key)
    }

    pub(crate) fn selected_row(&self) -> Option<&AgentRow> {
        self.rows.selected_item()
    }

    /// The highlighted agent, or `None` when the "New agent" row (or nothing) is
    /// highlighted.
    pub(crate) fn selected_agent(&self) -> Option<&AgentChoice> {
        match self.rows.selected_item() {
            Some(AgentRow::Agent(choice)) => Some(choice),
            _ => None,
        }
    }

    /// Opens the delete confirmation for the highlighted agent — but never for the
    /// active agent (its home is live) or the `default` base agent. No-op otherwise.
    pub(crate) fn begin_delete(&mut self, default_label: &str) {
        if let Some(agent) = self.selected_agent() {
            if agent.active || agent.name == default_label {
                return;
            }
            self.confirm_delete = Some(AgentDelete {
                name: agent.name.clone(),
            });
        }
    }
}

/// An editable field of the agent detail/edit overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentField {
    Description,
    Provider,
    Model,
    Dispatch,
    Persona,
}

/// The loaded values for an agent's detail/edit overlay, read off disk by the
/// event loop (config.toml + persona.md + agent.toml + memory db size).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgentDetailData {
    pub(crate) description: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    /// Comma-separated dispatch fallback routes (`agent.fallbacks`).
    pub(crate) dispatch: String,
    pub(crate) persona: String,
    /// Read-only one-line memory summary (e.g. `memory.sqlite · 1.2 MB`).
    pub(crate) memory_note: String,
}

/// The agent detail/edit overlay: shows an agent's default provider+model, dispatch
/// routes, persona, description, and a memory summary, and edits them inline. Opened
/// with `e` on an agent row; saving writes config.toml / persona.md / agent.toml.
#[derive(Clone, Debug)]
pub(crate) struct AgentDetail {
    pub(crate) name: String,
    /// The base/default agent has no `agent.toml`, so its description isn't editable.
    pub(crate) is_default: bool,
    pub(crate) description: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) dispatch: String,
    pub(crate) persona: String,
    pub(crate) memory_note: String,
    pub(crate) field: AgentField,
    /// `false` until the event loop delivers the on-disk values.
    pub(crate) loaded: bool,
    pub(crate) status: Option<String>,
}

impl AgentDetail {
    /// A pre-load placeholder shown while the event loop reads the agent's files.
    pub(crate) fn loading(name: String, is_default: bool) -> Self {
        Self {
            name,
            is_default,
            description: String::new(),
            provider: String::new(),
            model: String::new(),
            dispatch: String::new(),
            persona: String::new(),
            memory_note: String::new(),
            field: if is_default {
                AgentField::Provider
            } else {
                AgentField::Description
            },
            loaded: false,
            status: Some("Loading…".to_string()),
        }
    }

    /// Fills the buffers once the on-disk values arrive.
    pub(crate) fn apply(&mut self, data: AgentDetailData) {
        self.description = data.description;
        self.provider = data.provider;
        self.model = data.model;
        self.dispatch = data.dispatch;
        self.persona = data.persona;
        self.memory_note = data.memory_note;
        self.loaded = true;
        self.status = None;
    }

    /// The editable fields in display order (the default agent omits Description).
    pub(crate) fn fields(&self) -> Vec<AgentField> {
        let mut fields = Vec::new();
        if !self.is_default {
            fields.push(AgentField::Description);
        }
        fields.extend([
            AgentField::Provider,
            AgentField::Model,
            AgentField::Dispatch,
            AgentField::Persona,
        ]);
        fields
    }

    /// Moves focus to the next (`forward`) or previous field, wrapping.
    pub(crate) fn move_field(&mut self, forward: bool) {
        let fields = self.fields();
        let current = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        let len = fields.len();
        let next = if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
        self.field = fields[next];
    }

    /// The buffer for the currently focused field.
    pub(crate) fn active_buffer_mut(&mut self) -> &mut String {
        match self.field {
            AgentField::Description => &mut self.description,
            AgentField::Provider => &mut self.provider,
            AgentField::Model => &mut self.model,
            AgentField::Dispatch => &mut self.dispatch,
            AgentField::Persona => &mut self.persona,
        }
    }
}

/// Which field of the create-agent form is focused. The form is intentionally
/// small: a required name and an optional one-line description. Richer creation
/// (clone, model, persona file) lives in the `agent create` CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentCreateField {
    Name,
    Description,
}

/// The create-agent form overlay: a name field (required, validated via
/// `registry::validate_name`) and an optional description. Enter on a valid name
/// creates the agent via the registry and offers to switch to it; Tab moves
/// between fields; Esc cancels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentCreateForm {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) field: AgentCreateField,
    /// A validation / error message shown under the form (e.g. invalid name).
    pub(crate) error: Option<String>,
}

impl AgentCreateForm {
    pub(crate) fn new() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            field: AgentCreateField::Name,
            error: None,
        }
    }

    /// The buffer for the currently focused field.
    fn current_mut(&mut self) -> &mut String {
        match self.field {
            AgentCreateField::Name => &mut self.name,
            AgentCreateField::Description => &mut self.description,
        }
    }

    pub(crate) fn push(&mut self, c: char) {
        self.current_mut().push(c);
        self.error = None;
    }

    pub(crate) fn backspace(&mut self) {
        self.current_mut().pop();
        self.error = None;
    }

    pub(crate) fn focus_next(&mut self) {
        self.field = match self.field {
            AgentCreateField::Name => AgentCreateField::Description,
            AgentCreateField::Description => AgentCreateField::Name,
        };
    }
}

/// A top-level section of the Ctrl+P menu. Replaces the old flat action palette:
/// every action now lives under one of these four focused areas, so the launcher
/// stays short and scannable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuSection {
    /// The active agent + its roster: switch, create, edit, inspect.
    Agent,
    /// This agent's conversations: start a new one, resume, rename, delete.
    Conversations,
    /// Cloud organization: switch org, browse projects / members / data.
    Organization,
    /// Local instance settings: behavior, display, profiles, providers.
    Settings,
}

impl MenuSection {
    /// The sections in display order.
    pub(crate) const ORDER: [MenuSection; 4] = [
        MenuSection::Agent,
        MenuSection::Conversations,
        MenuSection::Organization,
        MenuSection::Settings,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            MenuSection::Agent => "Agent",
            MenuSection::Conversations => "Conversations",
            MenuSection::Organization => "Organization",
            MenuSection::Settings => "Settings",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            MenuSection::Agent => "switch · create · edit the active agent",
            MenuSection::Conversations => "new · resume · rename · delete chats",
            MenuSection::Organization => "switch org · projects · members · data",
            MenuSection::Settings => "behavior · display · profiles · providers",
        }
    }
}

impl PickerItem for MenuSection {
    fn label(&self) -> String {
        MenuSection::label(*self).to_string()
    }
    fn detail(&self) -> Option<String> {
        Some(MenuSection::hint(*self).to_string())
    }
}

/// The top-level Ctrl+P menu: four focused sections instead of one long list of
/// every action. Selecting a section opens its dedicated overlay.
#[derive(Clone, Debug)]
pub(crate) struct MainMenu {
    pub(crate) picker: Picker<MenuSection>,
}

impl Default for MainMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl MainMenu {
    pub(crate) fn new() -> Self {
        Self {
            picker: Picker::new(MenuSection::ORDER.to_vec()),
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.picker.on_key(key)
    }

    pub(crate) fn selected_section(&self) -> Option<MenuSection> {
        self.picker.selected_item().copied()
    }
}

/// A single toggleable preference shown in the main Settings overlay. These are
/// the everyday switches; the harness-loop internals live in the Advanced
/// sub-overlay (see [`AdvancedPref`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsPref {
    /// Show advanced status-bar info (model, real tokens, context meter).
    ShowAdvanced,
    /// Check for a newer codel00p release in the background on startup.
    CheckUpdates,
}

impl SettingsPref {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SettingsPref::ShowAdvanced => "Show advanced info",
            SettingsPref::CheckUpdates => "Check for updates on start",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            SettingsPref::ShowAdvanced => "model · token usage · context size",
            SettingsPref::CheckUpdates => "notify when a newer release is available",
        }
    }
}

/// One row in the main Settings overlay: either a toggleable preference or the
/// non-toggle "Advanced…" entry that opens the harness-knobs sub-overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsRow {
    Pref(SettingsPref),
    /// Cycles the tool-approval mode (`agent.permission_mode`): allow / ask / deny.
    PermissionMode,
    /// Cycles the active agent profile among the built-in presets + any
    /// user-defined `[agent.profiles.*]`, persisting `agent.profile`.
    Profile,
    /// Enter sets the active provider's API key (stored in the home `.env`).
    ApiKey,
    /// Read-only: the signed-in account (email · org · role), when authenticated.
    Account,
    /// Opens the Advanced settings sub-overlay (harness-loop internals).
    Advanced,
}

impl SettingsRow {
    /// All rows, in display order.
    pub(crate) const ORDER: [SettingsRow; 7] = [
        SettingsRow::Pref(SettingsPref::ShowAdvanced),
        SettingsRow::Pref(SettingsPref::CheckUpdates),
        SettingsRow::PermissionMode,
        SettingsRow::Profile,
        SettingsRow::ApiKey,
        SettingsRow::Account,
        SettingsRow::Advanced,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            SettingsRow::Pref(pref) => pref.label(),
            SettingsRow::PermissionMode => "Tool approvals",
            SettingsRow::Profile => "Agent profile",
            SettingsRow::ApiKey => "Provider API key",
            SettingsRow::Account => "Account",
            SettingsRow::Advanced => "Advanced…",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            SettingsRow::Pref(pref) => pref.hint(),
            SettingsRow::PermissionMode => "Enter/→ cycle · allow · ask · deny",
            SettingsRow::Profile => "Enter/→ cycle · autonomous · careful · manual",
            SettingsRow::ApiKey => "Enter to set the active provider's key (~/.env)",
            SettingsRow::Account => "signed-in user · organization · role",
            SettingsRow::Advanced => "harness-loop knobs · iteration count",
        }
    }
}

/// The Settings overlay: a list of TUI/agent preferences. Up/Down move, Enter/Space
/// act on the highlighted row, ←/→ cycle the choosers, Esc closes. When
/// `api_key_entry` is `Some`, the overlay is capturing a (masked) API key.
#[derive(Clone, Debug)]
pub(crate) struct SettingsOverlay {
    pub(crate) selected: usize,
    /// `Some(buffer)` while entering the active provider's API key.
    pub(crate) api_key_entry: Option<String>,
}

impl SettingsOverlay {
    pub(crate) fn new() -> Self {
        Self {
            selected: 0,
            api_key_entry: None,
        }
    }

    pub(crate) fn up(&mut self) {
        if self.selected == 0 {
            self.selected = SettingsRow::ORDER.len().saturating_sub(1);
        } else {
            self.selected -= 1;
        }
    }

    pub(crate) fn down(&mut self) {
        self.selected = (self.selected + 1) % SettingsRow::ORDER.len().max(1);
    }

    /// The currently highlighted row.
    pub(crate) fn current(&self) -> SettingsRow {
        SettingsRow::ORDER[self.selected.min(SettingsRow::ORDER.len() - 1)]
    }
}

/// One harness-internal preference in the Advanced sub-overlay. Each entry is
/// either a numeric knob (with step/min/max) or a boolean toggle, and carries
/// the dotted config key the harness already reads. These all require some
/// understanding of how the agent loop works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdvancedPref {
    /// `agent.max_iterations` — the agent-loop iteration ceiling.
    MaxIterations,
    /// `agent.behavior.verify_iterations` — verify→fix attempts before done.
    VerifyIterations,
    /// `agent.behavior.failure_budget` — same-op failures before the replan nudge.
    FailureBudget,
    /// `agent.behavior.self_knowledge` — inject the identity block each turn.
    SelfKnowledge,
    /// `agent.behavior.self_state` — include the live run-state line.
    SelfState,
    /// `agent.behavior.base_prompt` — inject the base operating prompt.
    BasePrompt,
    /// `agent.behavior.auto_plan` — include the base prompt's planning guidance.
    AutoPlan,
}

/// How an Advanced row is edited: a bounded integer (Left/Right or -/+) or a
/// boolean toggle (Enter/Space).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdvancedKind {
    /// A numeric knob edited by `step`, clamped to `[min, max]`.
    Number {
        step: u32,
        min: u32,
        max: u32,
    },
    Bool,
}

impl AdvancedPref {
    /// All advanced prefs, in display order. Numerics first (the headline ask),
    /// then the loop-internal toggles.
    pub(crate) const ORDER: [AdvancedPref; 7] = [
        AdvancedPref::MaxIterations,
        AdvancedPref::VerifyIterations,
        AdvancedPref::FailureBudget,
        AdvancedPref::SelfKnowledge,
        AdvancedPref::SelfState,
        AdvancedPref::BasePrompt,
        AdvancedPref::AutoPlan,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            AdvancedPref::MaxIterations => "Max iterations",
            AdvancedPref::VerifyIterations => "Verify iterations",
            AdvancedPref::FailureBudget => "Failure budget",
            AdvancedPref::SelfKnowledge => "Self-knowledge",
            AdvancedPref::SelfState => "Self run-state",
            AdvancedPref::BasePrompt => "Base operating prompt",
            AdvancedPref::AutoPlan => "Auto-plan guidance",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            AdvancedPref::MaxIterations => "max tool/model steps per turn",
            AdvancedPref::VerifyIterations => "verify→fix attempts before done",
            AdvancedPref::FailureBudget => "same-op failures before replan (0 = off)",
            AdvancedPref::SelfKnowledge => "inject identity · capabilities each turn",
            AdvancedPref::SelfState => "iteration · context · plan progress",
            AdvancedPref::BasePrompt => "understand · plan · verify before done",
            AdvancedPref::AutoPlan => "ask the agent to plan multi-step work",
        }
    }

    /// The dotted config key the harness reads for this preference.
    pub(crate) fn config_key(self) -> &'static str {
        match self {
            AdvancedPref::MaxIterations => "agent.max_iterations",
            AdvancedPref::VerifyIterations => "agent.behavior.verify_iterations",
            AdvancedPref::FailureBudget => "agent.behavior.failure_budget",
            AdvancedPref::SelfKnowledge => "agent.behavior.self_knowledge",
            AdvancedPref::SelfState => "agent.behavior.self_state",
            AdvancedPref::BasePrompt => "agent.behavior.base_prompt",
            AdvancedPref::AutoPlan => "agent.behavior.auto_plan",
        }
    }

    /// How this preference is edited (numeric step/bounds, or boolean toggle).
    pub(crate) fn kind(self) -> AdvancedKind {
        match self {
            // The iteration ceiling can't drop below 1, and a generous cap keeps
            // it sane.
            AdvancedPref::MaxIterations => AdvancedKind::Number {
                step: 1,
                min: 1,
                max: 200,
            },
            // At least one verify attempt; a small cap.
            AdvancedPref::VerifyIterations => AdvancedKind::Number {
                step: 1,
                min: 1,
                max: 20,
            },
            // 0 disables the replan nudge entirely.
            AdvancedPref::FailureBudget => AdvancedKind::Number {
                step: 1,
                min: 0,
                max: 20,
            },
            AdvancedPref::SelfKnowledge
            | AdvancedPref::SelfState
            | AdvancedPref::BasePrompt
            | AdvancedPref::AutoPlan => AdvancedKind::Bool,
        }
    }
}

/// The Advanced settings sub-overlay: harness-loop internals (the iteration
/// count and other knobs that require understanding the agent loop). Opened from
/// the main Settings overlay's "Advanced…" row; Esc returns there. Up/Down move,
/// Left/Right (or -/+) adjust a numeric row, Enter/Space toggle a boolean row.
#[derive(Clone, Debug)]
pub(crate) struct AdvancedSettingsOverlay {
    pub(crate) selected: usize,
}

impl AdvancedSettingsOverlay {
    pub(crate) fn new() -> Self {
        Self { selected: 0 }
    }

    pub(crate) fn up(&mut self) {
        if self.selected == 0 {
            self.selected = AdvancedPref::ORDER.len().saturating_sub(1);
        } else {
            self.selected -= 1;
        }
    }

    pub(crate) fn down(&mut self) {
        self.selected = (self.selected + 1) % AdvancedPref::ORDER.len().max(1);
    }

    /// The currently highlighted advanced preference.
    pub(crate) fn current(&self) -> AdvancedPref {
        AdvancedPref::ORDER[self.selected.min(AdvancedPref::ORDER.len() - 1)]
    }
}

/// The update-prompt panel shown when a live background check finds a newer
/// release: it offers "Update now" (Enter) and "Dismiss" (Esc), carrying the
/// running and latest versions for the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdatePrompt {
    pub(crate) current: String,
    pub(crate) latest: String,
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
    Menu(MainMenu),
    Settings(SettingsOverlay),
    AdvancedSettings(AdvancedSettingsOverlay),
    UpdatePrompt(UpdatePrompt),
    /// The local-agent switcher (multi-agent personas, #13).
    AgentSwitcher(AgentSwitcher),
    /// The create-agent form (multi-agent personas, #13).
    AgentCreate(AgentCreateForm),
    /// The agent detail/edit overlay (multi-agent personas, #13).
    AgentDetail(AgentDetail),
}

impl Overlay {
    pub(crate) fn is_open(&self) -> bool {
        !matches!(self, Overlay::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_summary_uses_title_as_primary_label() {
        let summary = SessionSummary {
            session_id: "chat-42".to_string(),
            title: Some("Review release blockers".to_string()),
            description: None,
            source: "cli".to_string(),
            message_count: 3,
        };

        assert_eq!(summary.label(), "Review release blockers");
        assert_eq!(
            summary.detail(),
            Some("chat-42 · cli · 3 message(s)".to_string())
        );
    }

    #[test]
    fn session_summary_detail_appends_description() {
        let summary = SessionSummary {
            session_id: "chat-9".to_string(),
            title: Some("Release prep".to_string()),
            description: Some("track the v0.13 cut".to_string()),
            source: "cli".to_string(),
            message_count: 1,
        };
        assert_eq!(
            summary.detail(),
            Some("chat-9 · cli · 1 message(s) — track the v0.13 cut".to_string())
        );
    }

    #[test]
    fn new_conversation_is_always_the_first_row() {
        let mut switcher = SessionSwitcher::new();
        switcher.set_sessions(
            vec![SessionSummary {
                session_id: "chat-1".to_string(),
                title: Some("First".to_string()),
                description: None,
                source: "cli".to_string(),
                message_count: 1,
            }],
            None,
        );
        // Row 0 is New (no session); selecting it isn't a session.
        assert!(matches!(
            switcher.selected_row(),
            Some(ConversationRow::New)
        ));
        assert!(switcher.selected_session().is_none());
    }
}
