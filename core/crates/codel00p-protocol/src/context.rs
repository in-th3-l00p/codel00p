//! Context-window pressure, compaction, and tool progress contracts.

use serde::{Deserialize, Serialize};

use crate::{EventId, SessionId, TurnId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextWindowState {
    model: String,
    context_limit_tokens: u64,
    used_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning_threshold_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocking_threshold_tokens: Option<u64>,
}

impl ContextWindowState {
    pub fn new(model: impl Into<String>, context_limit_tokens: u64, used_tokens: u64) -> Self {
        Self {
            model: model.into(),
            context_limit_tokens,
            used_tokens,
            warning_threshold_tokens: None,
            blocking_threshold_tokens: None,
        }
    }

    pub fn with_warning_threshold(mut self, tokens: u64) -> Self {
        self.warning_threshold_tokens = Some(tokens);
        self
    }

    pub fn with_blocking_threshold(mut self, tokens: u64) -> Self {
        self.blocking_threshold_tokens = Some(tokens);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn context_limit_tokens(&self) -> u64 {
        self.context_limit_tokens
    }

    pub fn used_tokens(&self) -> u64 {
        self.used_tokens
    }

    pub fn is_above_warning_threshold(&self) -> bool {
        self.warning_threshold_tokens
            .map(|threshold| self.used_tokens >= threshold)
            .unwrap_or(false)
    }

    pub fn is_at_blocking_limit(&self) -> bool {
        self.blocking_threshold_tokens
            .map(|threshold| self.used_tokens >= threshold)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionRecord {
    event_id: EventId,
    session_id: SessionId,
    turn_id: TurnId,
    before_message_count: usize,
    after_message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

impl CompactionRecord {
    pub fn new(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        before_message_count: usize,
        after_message_count: usize,
    ) -> Self {
        Self {
            event_id,
            session_id,
            turn_id,
            before_message_count,
            after_message_count,
            summary: None,
        }
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn before_message_count(&self) -> usize {
        self.before_message_count
    }

    pub fn after_message_count(&self) -> usize {
        self.after_message_count
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProgress {
    event_id: EventId,
    session_id: SessionId,
    turn_id: TurnId,
    tool_name: String,
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl ToolProgress {
    pub fn new(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
        phase: impl Into<String>,
    ) -> Self {
        Self {
            event_id,
            session_id,
            turn_id,
            tool_name: tool_name.into(),
            phase: phase.into(),
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn phase(&self) -> &str {
        &self.phase
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}
