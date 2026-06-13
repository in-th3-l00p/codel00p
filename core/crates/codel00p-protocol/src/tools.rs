//! Structured tool call and tool result payload contracts.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    id: String,
    name: String,
    input: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input(&self) -> &Value {
        &self.input
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    call_id: String,
    name: String,
    output: Value,
    success: bool,
}

impl ToolResult {
    pub fn success(call_id: impl Into<String>, name: impl Into<String>, output: Value) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            output,
            success: true,
        }
    }

    pub fn failure(
        call_id: impl Into<String>,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            output: serde_json::json!({ "error": message.into() }),
            success: false,
        }
    }

    pub fn output(&self) -> &Value {
        &self.output
    }

    pub fn success_status(&self) -> bool {
        self.success
    }
}
