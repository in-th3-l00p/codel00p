//! Permission request and decision contracts for tool execution.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{SessionId, TurnId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    ReadOnly,
    WorkspaceWrite,
    Shell,
    Network,
    ExternalConnector,
    MemoryWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    request_id: String,
    session_id: SessionId,
    turn_id: TurnId,
    tool_name: String,
    input: Value,
    scope: PermissionScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl PermissionRequest {
    pub fn new(
        request_id: impl Into<String>,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
        input: Value,
        scope: PermissionScope,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            session_id,
            turn_id,
            tool_name: tool_name.into(),
            input,
            scope,
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn id(&self) -> &str {
        &self.request_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn input(&self) -> &Value {
        &self.input
    }

    pub fn scope(&self) -> PermissionScope {
        self.scope
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDecision {
    request_id: String,
    mode: PermissionMode,
    status: PermissionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl PermissionDecision {
    pub fn allow(request_id: impl Into<String>, mode: PermissionMode) -> Self {
        Self {
            request_id: request_id.into(),
            mode,
            status: PermissionStatus::Allow,
            message: None,
        }
    }

    pub fn deny(
        request_id: impl Into<String>,
        mode: PermissionMode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            mode,
            status: PermissionStatus::Deny,
            message: Some(message.into()),
        }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    pub fn status(&self) -> PermissionStatus {
        self.status
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn allows_execution(&self) -> bool {
        self.status == PermissionStatus::Allow
    }
}
