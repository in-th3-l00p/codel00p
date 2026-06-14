//! MCP configuration parsing, diagnostics, and tool registry wiring for agent runs.

use super::*;

mod cli;
mod clients;
mod diagnostics;
mod registry;
mod spec;

pub(super) use cli::agent_mcp;
pub(super) use registry::build_mcp_registry_for_server;
pub(crate) use spec::{McpServerSpec, load_mcp_servers_from_workspace, parse_mcp_server};
