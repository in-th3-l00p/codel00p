//! Messages that drive the update loop and effects it produces. `Msg` is the only
//! way state changes; `Effect` is the only way the loop touches the outside world.

use codel00p_harness::{
    HarnessEvent, PermissionDecision, PermissionRequest, SessionState, TurnOutcome,
};
use codel00p_protocol::{Agent, McpServer, MemoryEntry, OrgMember, OrgRef, Project, Viewer};
use crossterm::event::KeyEvent;
use tokio::sync::oneshot;

use super::overlay::{AgentChoice, ModelChoice, SessionSummary};

/// Inbound events: terminal input, streamed turn output, and async results.
pub(crate) enum Msg {
    Key(KeyEvent),
    Resize,
    Tick,
    /// Scroll the transcript by a number of rows (positive = up/older, negative =
    /// down/newer). Emitted by the mouse wheel.
    Scroll(i16),
    /// A streamed assistant text token.
    Token(String),
    /// A structured harness event (tool lifecycle, compaction, etc.).
    Event(HarnessEvent),
    /// The spawned turn finished (or failed with a humanized message).
    TurnFinished(Result<Box<TurnOutcome>, String>),
    /// A tool needs Ask-mode approval; reply over the channel to unblock it.
    PermissionAsked(PermissionRequest, oneshot::Sender<PermissionDecision>),
    /// Text to surface as a notice block (e.g. from a `/sessions` lookup).
    Notice(String),
    CloudViewer(Result<Viewer, String>),
    CloudOrgs(Result<Vec<OrgRef>, String>),
    CloudProjects(Result<Vec<Project>, String>),
    CloudAgents(Result<Vec<Agent>, String>),
    CloudMcp(Result<Vec<McpServer>, String>),
    CloudMemory(Result<Vec<MemoryEntry>, String>),
    CloudUsers(Result<Vec<OrgMember>, String>),
    /// Live provider model catalog for the model picker (falls back to the static
    /// catalog on error / empty).
    Models(Result<Vec<ModelChoice>, String>),
    /// Prior sessions for the session switcher overlay.
    SessionList(Result<Vec<SessionSummary>, String>),
    /// Local agents (default + registry) for the agent switcher overlay
    /// (multi-agent personas, #13).
    AgentList(Result<Vec<AgentChoice>, String>),
    /// A prior session was replayed and is ready to resume (state + message count).
    SessionResumed(Result<(Box<SessionState>, usize), String>),
    /// The background startup check found a newer release (carries the version).
    /// Opens the update-prompt panel unless already dismissed this session.
    UpdateAvailable(String),
    /// Start the background update check (dispatched once at startup). Produces an
    /// `Effect::CheckUpdates` so the network call runs off the UI task.
    CheckUpdates,
}

/// Side effects the event loop executes after an update.
pub(crate) enum Effect {
    /// Run a turn with the given prompt against the current options.
    SubmitTurn(String),
    /// Send a permission decision back to the waiting tool.
    ResolvePermission(oneshot::Sender<PermissionDecision>, PermissionDecision),
    /// Persist a completed turn's new messages and events, starting at the given
    /// prior message count (the persistence "start index").
    Persist(Box<TurnOutcome>, usize),
    /// Fetch cloud data (blocking client, run off the UI task).
    Cloud(CloudFetch),
    /// Re-authenticate through the browser for a selected organization.
    SwitchOrg(String),
    /// Read local state and surface it as a notice.
    Local(LocalQuery),
    /// Fetch the provider model catalog for the picker (blocking client, off the UI
    /// task). Carries the provider whose models to list.
    FetchModels(String),
    /// List prior sessions for the switcher (blocking store read, off the UI task).
    ListSessions,
    /// List local agents for the switcher (blocking registry scan, off the UI
    /// task). Multi-agent personas, #13.
    ListAgents,
    /// Live-switch the active agent (multi-agent personas, #13): persist the
    /// sticky pointer, re-point the running TUI's `CODEL00P_HOME` + rebuild
    /// `App.config` so the next harness build uses the new agent's memory +
    /// sessions, reset to a fresh session under it, and reload the session
    /// switcher. Runs synchronously on the UI task (only when idle / between
    /// turns), so the env mutation is single-threaded with no harness in flight.
    SwitchAgent(String),
    /// Replay a prior session so it can be resumed inside the TUI.
    ResumeSession(codel00p_harness::SessionId),
    /// Apply a conversation's edited name + description (blocking store writes off
    /// the UI task), then refresh the switcher list. An empty description clears it.
    EditSession {
        session_id: codel00p_harness::SessionId,
        title: String,
        description: String,
    },
    /// Delete a conversation (blocking store write off the UI task), then refresh
    /// the switcher list.
    DeleteSession(codel00p_harness::SessionId),
    /// Run a live update check in the background (off the UI task) and notify this
    /// session via `Msg::UpdateAvailable` if a newer release is found. Dispatched
    /// once at startup when checking is enabled; never blocks the first render.
    CheckUpdates,
    /// Leave the TUI.
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CloudFetch {
    Viewer,
    Orgs,
    Projects,
    Agents(String),
    Mcp(String),
    Memory(String),
    Users,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalQuery {
    Sessions,
    Memory,
}
