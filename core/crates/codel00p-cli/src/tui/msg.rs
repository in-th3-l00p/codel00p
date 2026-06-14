//! Messages that drive the update loop and effects it produces. `Msg` is the only
//! way state changes; `Effect` is the only way the loop touches the outside world.

use codel00p_harness::{HarnessEvent, PermissionDecision, PermissionRequest, TurnOutcome};
use codel00p_protocol::{Agent, McpServer, MemoryEntry, Project, Viewer};
use crossterm::event::KeyEvent;
use tokio::sync::oneshot;

/// Inbound events: terminal input, streamed turn output, and async results.
pub(crate) enum Msg {
    Key(KeyEvent),
    Resize,
    Tick,
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
    CloudProjects(Result<Vec<Project>, String>),
    CloudAgents(Result<Vec<Agent>, String>),
    CloudMcp(Result<Vec<McpServer>, String>),
    CloudMemory(Result<Vec<MemoryEntry>, String>),
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
    /// Read local state and surface it as a notice.
    Local(LocalQuery),
    /// Leave the TUI.
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CloudFetch {
    Viewer,
    Projects,
    Agents(String),
    Mcp(String),
    Memory(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalQuery {
    Sessions,
    Memory,
}
