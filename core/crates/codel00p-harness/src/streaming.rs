//! Streaming sink adapter that fans provider stream callbacks out to the
//! caller's token sink and, for tool-call argument deltas, onto the harness
//! event stream as [`AgentEvent::ToolCallArgsDelta`].
//!
//! The provider transports invoke [`TokenSink`] callbacks synchronously from
//! inside the response read loop, but the harness [`AgentEventSink`] is async.
//! To surface tool-call deltas as live events without blocking the read loop,
//! the adapter pushes each delta onto an unbounded in-memory channel; a drain
//! future ([`StreamEventForwarder::forward`]) runs concurrently with the
//! inference call and emits the buffered events through the event sink as they
//! arrive. This keeps the new events additive and ordered while leaving the
//! final assembled tool calls (and all non-streaming behavior) untouched.

use std::sync::Arc;

use codel00p_protocol::{EventId, SessionId, TurnId};
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
};

use crate::{event_sink::AgentEventSink, events::HarnessEvent, turn::TokenSink};

/// A buffered tool-call argument delta, carried from the synchronous transport
/// callback to the async forwarder.
struct ToolCallDelta {
    index: usize,
    id: Option<String>,
    name: Option<String>,
    args_fragment: String,
}

/// Wraps the caller's optional [`TokenSink`] and, when an event sink is present,
/// captures tool-call argument deltas for forwarding as harness events.
///
/// `on_token` (and `on_tool_call_delta`) are always forwarded to the inner
/// token sink unchanged, so existing streaming behavior is preserved.
pub(crate) struct StreamingSink {
    inner: Option<Arc<dyn TokenSink>>,
    delta_tx: Option<UnboundedSender<ToolCallDelta>>,
}

impl StreamingSink {
    /// Builds a streaming sink plus, when an event sink is wired, a forwarder
    /// that turns captured tool-call deltas into [`HarnessEvent`]s.
    ///
    /// Returns `(sink, forwarder)`. Run `forwarder.forward()` concurrently with
    /// the inference call; it completes once the sink is dropped and the channel
    /// drains, so the streaming call must finish (dropping the sink) for the
    /// forwarder to return.
    pub(crate) fn new(
        token_sink: Option<Arc<dyn TokenSink>>,
        event_sink: Option<Arc<dyn AgentEventSink>>,
        session_id: SessionId,
        turn_id: TurnId,
    ) -> (Self, StreamEventForwarder) {
        match event_sink {
            Some(event_sink) => {
                let (tx, rx) = unbounded();
                (
                    Self {
                        inner: token_sink,
                        delta_tx: Some(tx),
                    },
                    StreamEventForwarder {
                        rx: Some(rx),
                        event_sink: Some(event_sink),
                        session_id,
                        turn_id,
                    },
                )
            }
            None => (
                Self {
                    inner: token_sink,
                    delta_tx: None,
                },
                StreamEventForwarder {
                    rx: None,
                    event_sink: None,
                    session_id,
                    turn_id,
                },
            ),
        }
    }
}

impl TokenSink for StreamingSink {
    fn on_token(&self, token: &str) {
        if let Some(inner) = &self.inner {
            inner.on_token(token);
        }
    }

    fn on_tool_call_delta(
        &self,
        index: usize,
        id: Option<&str>,
        name: Option<&str>,
        args_fragment: &str,
    ) {
        if let Some(inner) = &self.inner {
            inner.on_tool_call_delta(index, id, name, args_fragment);
        }
        if let Some(tx) = &self.delta_tx {
            // Unbounded send only fails once the receiver is dropped, which only
            // happens after the forwarder has finished draining; dropping a late
            // delta in that window is harmless (the final call is unaffected).
            let _ = tx.unbounded_send(ToolCallDelta {
                index,
                id: id.map(str::to_string),
                name: name.map(str::to_string),
                args_fragment: args_fragment.to_string(),
            });
        }
    }
}

/// Drains buffered tool-call deltas and emits them as [`HarnessEvent`]s.
pub(crate) struct StreamEventForwarder {
    rx: Option<UnboundedReceiver<ToolCallDelta>>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
    session_id: SessionId,
    turn_id: TurnId,
}

impl StreamEventForwarder {
    /// Runs until the paired [`StreamingSink`] is dropped and the channel is
    /// empty, emitting one [`HarnessEvent::ToolCallArgsDelta`] per captured
    /// delta. A no-op when no event sink was wired.
    pub(crate) async fn forward(self) {
        let (Some(mut rx), Some(event_sink)) = (self.rx, self.event_sink) else {
            return;
        };
        while let Some(delta) = rx.next().await {
            let event = HarnessEvent::ToolCallArgsDelta {
                event_id: EventId::new(),
                session_id: self.session_id.clone(),
                turn_id: self.turn_id.clone(),
                index: delta.index,
                id: delta.id,
                name: delta.name,
                args_fragment: delta.args_fragment,
            };
            event_sink.emit(&event).await;
        }
    }
}
