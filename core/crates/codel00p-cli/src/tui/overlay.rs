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
    pub(crate) title: Option<String>,
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
        if self.title.is_some() {
            Some(format!(
                "{} · {} · {} message(s)",
                self.session_id, self.source, self.message_count
            ))
        } else {
            Some(format!(
                "{} · {} message(s)",
                self.source, self.message_count
            ))
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionRename {
    pub(crate) session_id: String,
    pub(crate) input: String,
}

/// The session switcher: a read-only list of prior conversations plus a status line
/// (loading / error). Selecting a row resumes that session in place; pressing the
/// rename key (F2) on a row enters an inline rename mode.
#[derive(Clone, Debug)]
pub(crate) struct SessionSwitcher {
    pub(crate) sessions: Picker<SessionSummary>,
    pub(crate) status: Option<String>,
    /// `Some` while the user is editing a session's title inline.
    pub(crate) rename: Option<SessionRename>,
}

impl SessionSwitcher {
    pub(crate) fn new() -> Self {
        Self {
            sessions: Picker::new(Vec::new()),
            status: Some("Loading…".to_string()),
            rename: None,
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

    /// Enters rename mode for the highlighted session, seeding the input with its
    /// current title (falling back to the id). No-op if no row is highlighted.
    pub(crate) fn begin_rename(&mut self) {
        if let Some(session) = self.sessions.selected_item() {
            let input = session
                .title
                .clone()
                .unwrap_or_else(|| session.session_id.clone());
            self.rename = Some(SessionRename {
                session_id: session.session_id.clone(),
                input,
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

/// The agent switcher overlay: a read-only list of local agents (default + every
/// `<base>/agents/<name>`). Selecting one performs a LIVE switch — it re-points
/// the running TUI at that agent's home so subsequent turns use its memory and
/// sessions. Mirrors [`SessionSwitcher`]'s shape.
#[derive(Clone, Debug)]
pub(crate) struct AgentSwitcher {
    pub(crate) agents: Picker<AgentChoice>,
    pub(crate) status: Option<String>,
}

impl AgentSwitcher {
    pub(crate) fn new() -> Self {
        Self {
            agents: Picker::new(Vec::new()),
            status: Some("Loading…".to_string()),
        }
    }

    pub(crate) fn set_agents(&mut self, agents: Vec<AgentChoice>, status: Option<String>) {
        self.agents.set_items(agents);
        self.status = status;
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        self.agents.on_key(key)
    }

    pub(crate) fn selected_item(&self) -> Option<&AgentChoice> {
        self.agents.selected_item()
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

/// An action reachable from the command palette. Each maps to an existing handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandAction {
    Model,
    Sessions,
    NewConversation,
    /// Open the local-agent switcher (multi-agent personas, #13).
    SwitchAgent,
    /// Open the create-agent form (multi-agent personas, #13).
    CreateAgent,
    Browse,
    Users,
    SwitchOrg,
    History,
    Tools,
    Settings,
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
            "Switch agent",
            "live-switch the active agent + memory",
            SwitchAgent,
        ),
        ("Create agent", "define a new local agent", CreateAgent),
        (
            "Browse organization",
            "projects · agents · MCP · memory",
            Browse,
        ),
        ("Browse users", "organization members", Users),
        ("Switch organization", "re-auth into another org", SwitchOrg),
        ("Show history", "this conversation's messages", History),
        ("Show tools", "enabled tool sets", Tools),
        ("Settings", "TUI preferences", Settings),
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
    /// Cycles the active agent profile among the built-in presets + any
    /// user-defined `[agent.profiles.*]`, persisting `agent.profile`.
    Profile,
    /// Opens the Advanced settings sub-overlay (harness-loop internals).
    Advanced,
}

impl SettingsRow {
    /// All rows, in display order. The toggle prefs first, then the profile
    /// switcher, then "Advanced…".
    pub(crate) const ORDER: [SettingsRow; 4] = [
        SettingsRow::Pref(SettingsPref::ShowAdvanced),
        SettingsRow::Pref(SettingsPref::CheckUpdates),
        SettingsRow::Profile,
        SettingsRow::Advanced,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            SettingsRow::Pref(pref) => pref.label(),
            SettingsRow::Profile => "Agent profile",
            SettingsRow::Advanced => "Advanced…",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            SettingsRow::Pref(pref) => pref.hint(),
            SettingsRow::Profile => "Enter/→ cycle · autonomous · careful · manual",
            SettingsRow::Advanced => "harness-loop knobs · iteration count",
        }
    }
}

/// The Settings overlay: a small list of toggleable TUI preferences plus an
/// "Advanced…" entry. Up/Down move the selection, Enter/Space toggle the
/// highlighted preference (or open Advanced), Esc closes.
#[derive(Clone, Debug)]
pub(crate) struct SettingsOverlay {
    pub(crate) selected: usize,
}

impl SettingsOverlay {
    pub(crate) fn new() -> Self {
        Self { selected: 0 }
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
    Command(CommandPalette),
    Settings(SettingsOverlay),
    AdvancedSettings(AdvancedSettingsOverlay),
    UpdatePrompt(UpdatePrompt),
    /// The local-agent switcher (multi-agent personas, #13).
    AgentSwitcher(AgentSwitcher),
    /// The create-agent form (multi-agent personas, #13).
    AgentCreate(AgentCreateForm),
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
            source: "cli".to_string(),
            message_count: 3,
        };

        assert_eq!(summary.label(), "Review release blockers");
        assert_eq!(
            summary.detail(),
            Some("chat-42 · cli · 3 message(s)".to_string())
        );
    }
}
