//! Error types shared by MCP clients, servers, and harness tools.

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("mcp client failed: {message}")]
    Client { message: String },

    #[error("mcp tool failed: {server_id}/{tool_name}: {message}")]
    Tool {
        server_id: String,
        tool_name: String,
        message: String,
    },

    #[error("invalid stdio transport message: {message}")]
    InvalidStdioMessage { message: String },

    #[error("mcp stdio transport failed for {server_id}: {message}")]
    StdioTransport { server_id: String, message: String },

    #[error("mcp http transport failed for {server_id}: {message}")]
    HttpTransport { server_id: String, message: String },
}
