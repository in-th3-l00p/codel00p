//! Built-in tool-set assembly before plugin and MCP tools are added.

use super::*;

/// When the combined MCP tool set is larger than this, its tools are folded in
/// as *deferred* (progressive disclosure): the model sees the built-in tools
/// plus `tool_search` / `tool_describe`, and loads MCP tool schemas on demand
/// instead of paying their full prompt cost up front. Small MCP setups stay
/// fully advertised so nothing changes for the common case.
const MCP_DISCLOSURE_THRESHOLD: usize = 15;

pub(super) async fn build_tool_registry(
    tool_sets: &[AgentToolSet],
    mcp_servers: &[McpServerSpec],
) -> CliResult<ToolRegistry> {
    let mut registry = ToolRegistry::read_only_defaults();
    for tool_set in tool_sets {
        registry = match tool_set {
            AgentToolSet::Read => registry,
            AgentToolSet::Edit => registry.with_registry(ToolRegistry::editing_defaults()),
            AgentToolSet::Command => registry.with_registry(ToolRegistry::command_defaults()),
            AgentToolSet::Git => registry.with_registry(ToolRegistry::git_defaults()),
            AgentToolSet::Web => registry.with_registry(ToolRegistry::web_defaults()),
            // Delegation needs the provider/model config to build a spawner, so
            // it is folded in by `build_agent_harness`, not here.
            AgentToolSet::Delegate => registry,
            // Learning needs the skills directory to record proposals, so it is
            // folded in by `build_agent_harness`, not here.
            AgentToolSet::Learn => registry,
            AgentToolSet::All => registry
                .with_registry(ToolRegistry::editing_defaults())
                .with_registry(ToolRegistry::command_defaults())
                .with_registry(ToolRegistry::git_defaults())
                .with_registry(ToolRegistry::web_defaults()),
        };
    }

    // Combine every MCP server's tools into one registry so disclosure is
    // decided on the aggregate, not per server.
    let mut mcp = ToolRegistry::new();
    for server in mcp_servers {
        mcp = mcp.with_registry(build_mcp_registry_for_server(server).await?);
    }

    registry = if mcp.names().len() > MCP_DISCLOSURE_THRESHOLD {
        registry.with_deferred_registry(mcp)
    } else {
        registry.with_registry(mcp)
    };

    Ok(registry)
}
