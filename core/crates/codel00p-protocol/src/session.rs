//! Session transcript message contracts used by harnesses and persistence.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ToolCall;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionMessage {
    role: SessionRole,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
}

impl SessionMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(SessionRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text(SessionRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(SessionRole::Assistant, content)
    }

    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: SessionRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_name: None,
            payload: None,
            tool_calls,
        }
    }

    pub fn tool(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        payload: Value,
    ) -> Self {
        let content = payload.to_string();
        Self::tool_result_parts(tool_call_id, tool_name, content, Some(payload))
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::tool_result_parts(tool_call_id, tool_name, content, None)
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        self.tool_call_id.as_deref()
    }

    pub fn tool_name(&self) -> Option<&str> {
        self.tool_name.as_deref()
    }

    fn tool_result_parts(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
        payload: Option<Value>,
    ) -> Self {
        Self {
            role: SessionRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_name: Some(tool_name.into()),
            payload,
            tool_calls: Vec::new(),
        }
    }

    pub fn role(&self) -> SessionRole {
        self.role
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        &self.tool_calls
    }

    fn text(role: SessionRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
            payload: None,
            tool_calls: Vec::new(),
        }
    }
}
