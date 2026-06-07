use serde::{Deserialize, Serialize};

use codel00p_protocol::ToolCall;
pub use codel00p_protocol::{SessionId, SessionMessage, TurnId};

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

    pub fn push_assistant_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        self.messages
            .push(SessionMessage::assistant_tool_calls(tool_calls));
    }

    pub fn push_tool_result(
        &mut self,
        call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) {
        self.messages
            .push(SessionMessage::tool_result(call_id, name, content));
    }
}
