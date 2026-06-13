//! Client capability advertisement helpers shared by MCP transports.

use serde_json::Value;

use crate::McpClientRoot;

pub(crate) fn client_capabilities(roots: &[McpClientRoot]) -> Value {
    let mut capabilities = serde_json::Map::new();
    if !roots.is_empty() {
        capabilities.insert(
            "roots".to_string(),
            serde_json::json!({
                "listChanged": false
            }),
        );
    }
    Value::Object(capabilities)
}
