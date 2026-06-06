use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
static TURN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn new() -> Self {
        let id = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("session-{id}"))
    }

    pub fn from_static(value: &'static str) -> Self {
        Self(value.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(String);

impl TurnId {
    pub fn new() -> Self {
        let id = TURN_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("turn-{id}"))
    }

    pub fn from_static(value: &'static str) -> Self {
        Self(value.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TurnId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMessage {
    content: String,
}

impl UserMessage {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    Tool {
        call_id: String,
        name: String,
        content: String,
    },
}

impl SessionMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: content.into(),
        }
    }

    pub fn tool(
        call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::Tool {
            call_id: call_id.into(),
            name: name.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    session_id: SessionId,
    messages: Vec<SessionMessage>,
}

impl SessionState {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            messages: Vec::new(),
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn messages(&self) -> &[SessionMessage] {
        &self.messages
    }

    pub fn push_user(&mut self, message: UserMessage) {
        self.messages.push(SessionMessage::user(message.content));
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.messages.push(SessionMessage::assistant(content));
    }

    pub fn push_tool_result(
        &mut self,
        call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) {
        self.messages
            .push(SessionMessage::tool(call_id, name, content));
    }
}
