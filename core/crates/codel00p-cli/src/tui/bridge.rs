//! Bridges the async harness turn to the TUI. The harness streams tokens, emits
//! events, and asks for permission on its own task; these adapters forward each
//! onto the UI's `mpsc<Msg>` channel so the render loop never blocks on the turn,
//! and the turn never blocks on the UI.

use async_trait::async_trait;
use codel00p_harness::{
    AgentEventSink, HarnessError, HarnessEvent, PermissionDecision, PermissionPolicy,
    PermissionRequest, TokenSink,
};
use tokio::sync::{mpsc, oneshot};

use crate::agent::CliPermissionPolicy;

use super::msg::Msg;

/// Forwards streamed assistant tokens to the UI. Uses an unbounded channel so the
/// harness never stalls waiting for the render loop.
#[derive(Clone)]
pub(crate) struct ChannelTokenSink {
    tx: mpsc::UnboundedSender<Msg>,
}

impl ChannelTokenSink {
    pub(crate) fn new(tx: mpsc::UnboundedSender<Msg>) -> Self {
        Self { tx }
    }
}

impl TokenSink for ChannelTokenSink {
    fn on_token(&self, token: &str) {
        let _ = self.tx.send(Msg::Token(token.to_string()));
    }
}

/// Forwards structured harness events (tool lifecycle, compaction, …) to the UI.
#[derive(Clone)]
pub(crate) struct ChannelEventSink {
    tx: mpsc::UnboundedSender<Msg>,
}

impl ChannelEventSink {
    pub(crate) fn new(tx: mpsc::UnboundedSender<Msg>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl AgentEventSink for ChannelEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        let _ = self.tx.send(Msg::Event(event.clone()));
    }
}

/// Routes Ask-mode permission requests to an in-app modal. `Allow`/`Deny` modes and
/// remembered decisions are resolved immediately by the wrapped `CliPermissionPolicy`
/// without touching the UI; otherwise it posts a request and awaits the user's
/// choice over a oneshot, so the tool pauses while the UI stays interactive.
pub(crate) struct TuiPermissionPolicy {
    inner: CliPermissionPolicy,
    ui_tx: mpsc::UnboundedSender<Msg>,
}

impl TuiPermissionPolicy {
    pub(crate) fn new(inner: CliPermissionPolicy, ui_tx: mpsc::UnboundedSender<Msg>) -> Self {
        Self { inner, ui_tx }
    }
}

#[async_trait]
impl PermissionPolicy for TuiPermissionPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        if let Some(decision) = self.inner.fast_path(&request)? {
            return Ok(decision);
        }

        let (reply_tx, reply_rx) = oneshot::channel();
        self.ui_tx
            .send(Msg::PermissionAsked(request.clone(), reply_tx))
            .map_err(|_| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: "permission UI is unavailable".to_string(),
            })?;

        let decision = reply_rx.await.map_err(|_| HarnessError::ToolFailed {
            name: request.tool_name().to_string(),
            message: "permission request was dropped before a decision".to_string(),
        })?;

        self.inner.persist_decision_if_needed(&request, &decision)?;
        Ok(decision)
    }
}
