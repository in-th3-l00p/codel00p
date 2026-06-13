//! Encoding and decoding for newline-delimited stdio JSON-RPC messages.

use crate::{JsonRpcMessage, McpError};

pub fn encode_stdio_message(message: &JsonRpcMessage) -> Result<String, McpError> {
    let encoded =
        serde_json::to_string(message).map_err(|error| McpError::InvalidStdioMessage {
            message: error.to_string(),
        })?;
    if encoded.contains('\n') || encoded.contains('\r') {
        return Err(McpError::InvalidStdioMessage {
            message: "stdio messages must not contain embedded newlines".to_string(),
        });
    }
    Ok(format!("{encoded}\n"))
}

pub fn decode_stdio_message(line: &str) -> Result<JsonRpcMessage, McpError> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(McpError::InvalidStdioMessage {
            message: "stdio messages must not contain embedded newlines".to_string(),
        });
    }
    serde_json::from_str(trimmed).map_err(|error| McpError::InvalidStdioMessage {
        message: error.to_string(),
    })
}
