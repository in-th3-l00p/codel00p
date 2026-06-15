//! Messages that drive the update loop and effects it produces. `Msg` is the only
//! way state changes; `Effect` is the only way the loop touches the outside world.

use codel00p_harness::{
    HarnessEvent, PermissionDecision, PermissionRequest, SessionState, TurnOutcome,
};
use codel00p_protocol::{Agent, McpServer, MemoryEntry, OrgMember, OrgRef, Project, Viewer};
use crossterm::event::KeyEvent;
use tokio::sync::oneshot;

use super::overlay::{ModelChoice, SessionSummary};

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
    /// A prior session was replayed and is ready to resume (state + message count).
    SessionResumed(Result<(Box<SessionState>, usize), String>),
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
    /// Replay a prior session so it can be resumed inside the TUI.
    ResumeSession(codel00p_harness::SessionId),
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
