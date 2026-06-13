//! MCP integration for codel00p.
//!
//! This crate provides protocol-neutral JSON-RPC types, stdio and Streamable HTTP
//! clients, lightweight server helpers, and a harness tool adapter for discovered
//! MCP tools. The public facade keeps the crate API stable while the transport,
//! parsing, and descriptor implementations live in focused modules.

mod capabilities;
mod descriptors;
mod error;
mod http_client;
mod initialization;
mod json_rpc;
mod notifications;
mod parsing;
mod server;
mod stdio_client;
mod stdio_codec;
mod subscriptions;
mod tool;

pub use descriptors::{
    McpClient, McpPromptArgument, McpPromptDescriptor, McpPromptMessage, McpPromptOutput,
    McpResourceContent, McpResourceDescriptor, McpResourceOutput, McpResourceTemplateDescriptor,
    McpToolCall, McpToolDescriptor, McpToolOutput,
};
pub use error::McpError;
pub use http_client::{HttpServerEndpoint, McpHttpClient};
pub use initialization::McpInitialization;
pub use json_rpc::{
    JsonRpcId, JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
};
pub(crate) use notifications::McpClientExchange;
pub use notifications::McpClientNotification;
pub use server::{McpServerHandler, McpServerResponse, McpServerRuntime, serve_stdio_server};
pub use stdio_client::{McpClientRoot, McpStdioClient, StdioServerCommand};
pub use stdio_codec::{decode_stdio_message, encode_stdio_message};
pub use subscriptions::{
    McpNotificationWorker, McpReconnectPolicy, McpStdioNotificationSupervisor, McpSubscriptionEvent,
};
pub use tool::{McpTool, discover_tool_registry};

pub fn crate_name() -> &'static str {
    "codel00p-mcp"
}
