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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCompactionRecord {
    before_message_count: usize,
    after_message_count: usize,
    summary: String,
}

impl SessionCompactionRecord {
    fn new(before_message_count: usize, after_message_count: usize, summary: String) -> Self {
        Self {
            before_message_count,
            after_message_count,
            summary,
        }
    }

    pub fn before_message_count(&self) -> usize {
        self.before_message_count
    }

    pub fn after_message_count(&self) -> usize {
        self.after_message_count
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }
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

    pub fn push_message(&mut self, message: SessionMessage) {
        self.messages.push(message);
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

    pub fn compact_with_summary(
        &mut self,
        summary: impl Into<String>,
        keep_recent_messages: usize,
    ) -> SessionCompactionRecord {
        let summary = summary.into();
        let before_message_count = self.messages.len();
        let keep_recent_messages = keep_recent_messages.min(before_message_count);
        let retained_start = before_message_count.saturating_sub(keep_recent_messages);
        let retained = self.messages.split_off(retained_start);

        self.messages = Vec::with_capacity(retained.len() + 1);
        self.messages.push(SessionMessage::system(format!(
            "Session summary:\n{}",
            summary
        )));
        self.messages.extend(retained);

        SessionCompactionRecord::new(before_message_count, self.messages.len(), summary)
    }
}
