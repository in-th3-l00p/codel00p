//! Agent runtime events emitted by harness execution.

use serde::{Deserialize, Serialize};

use crate::{EventId, PermissionScope, SessionId, TurnId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        event_id: EventId,
        session_id: SessionId,
    },
    TurnStarted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
    },
    ContextBuilt {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        message_count: usize,
    },
    ContextCompacted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        before_message_count: usize,
        after_message_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    InferenceRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        provider: String,
        model: String,
    },
    InferenceCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        finish_reason: Option<String>,
    },
    ToolCallRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
    },
    ToolCallCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
    },
    ToolCallFailed {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        message: String,
    },
    PermissionRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        request_id: String,
        scope: PermissionScope,
    },
    PermissionDenied {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        request_id: String,
        message: String,
    },
    ToolProgress {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        phase: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    LifecycleHookFailed {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        hook: String,
        message: String,
    },
    TurnCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        iterations: u32,
    },
}

impl AgentEvent {
    pub fn tool_call_completed(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
    ) -> Self {
        Self::ToolCallCompleted {
            event_id,
            session_id,
            turn_id,
            tool_name: tool_name.into(),
        }
    }
}
