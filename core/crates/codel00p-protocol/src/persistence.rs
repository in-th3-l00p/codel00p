//! Session persistence events emitted around append and replay operations.

use serde::{Deserialize, Serialize};

use crate::{EventId, SessionId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionPersistenceEvent {
    RecordAppended {
        event_id: EventId,
        session_id: SessionId,
        record_id: String,
        sequence: u64,
    },
    ReplayStarted {
        event_id: EventId,
        session_id: SessionId,
    },
    ReplayCompleted {
        event_id: EventId,
        session_id: SessionId,
        record_count: u64,
    },
}

impl SessionPersistenceEvent {
    pub fn record_appended(
        event_id: EventId,
        session_id: SessionId,
        record_id: impl Into<String>,
        sequence: u64,
    ) -> Self {
        Self::RecordAppended {
            event_id,
            session_id,
            record_id: record_id.into(),
            sequence,
        }
    }

    pub fn record_id(&self) -> &str {
        match self {
            Self::RecordAppended { record_id, .. } => record_id,
            Self::ReplayStarted { .. } | Self::ReplayCompleted { .. } => "",
        }
    }

    pub fn sequence(&self) -> u64 {
        match self {
            Self::RecordAppended { sequence, .. } => *sequence,
            Self::ReplayStarted { .. } | Self::ReplayCompleted { .. } => 0,
        }
    }
}
