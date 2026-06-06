use serde::{Deserialize, Serialize};

use crate::session::{SessionId, TurnId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarnessEvent {
    SessionStarted {
        session_id: SessionId,
    },
    TurnStarted {
        session_id: SessionId,
        turn_id: TurnId,
    },
    ContextBuilt {
        message_count: usize,
    },
    InferenceRequested {
        provider: String,
        model: String,
    },
    InferenceCompleted {
        finish_reason: Option<String>,
    },
    ToolCallRequested {
        name: String,
    },
    ToolCallCompleted {
        name: String,
    },
    ToolCallFailed {
        name: String,
        message: String,
    },
    TurnCompleted {
        iterations: u32,
    },
}
