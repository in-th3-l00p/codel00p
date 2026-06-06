use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    content: Value,
}

impl ToolResult {
    pub fn json(content: Value) -> Self {
        Self { content }
    }

    pub fn content(&self) -> &Value {
        &self.content
    }
}
