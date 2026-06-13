//! MCP initialization response data returned by server handshakes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpInitialization {
    pub(crate) protocol_version: String,
    pub(crate) capabilities: Value,
    pub(crate) server_info: Value,
    pub(crate) instructions: Option<String>,
}

impl McpInitialization {
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    pub fn server_info(&self) -> &Value {
        &self.server_info
    }

    pub fn server_name(&self) -> Option<&str> {
        self.server_info.get("name").and_then(Value::as_str)
    }

    pub fn server_version(&self) -> Option<&str> {
        self.server_info.get("version").and_then(Value::as_str)
    }

    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }
}
