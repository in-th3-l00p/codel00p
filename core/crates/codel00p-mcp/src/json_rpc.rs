//! Minimal JSON-RPC 2.0 message shapes used by MCP transports.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(u64),
    String(String),
}

impl From<u64> for JsonRpcId {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for JsonRpcId {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub(crate) jsonrpc: String,
    pub(crate) id: JsonRpcId,
    method: String,
    params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<JsonRpcId>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub(crate) jsonrpc: String,
    pub(crate) id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<Value>,
}

impl JsonRpcResponse {
    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub fn error(&self) -> Option<&Value> {
        self.error.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
    Response(JsonRpcResponse),
    Raw(Value),
}

impl JsonRpcMessage {
    pub fn request(request: JsonRpcRequest) -> Self {
        Self::Request(request)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    params: Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }
}
