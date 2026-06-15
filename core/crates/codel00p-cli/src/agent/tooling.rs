//! Built-in tool-set assembly before plugin and MCP tools are added.

use super::*;

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
    for server in mcp_servers {
        registry = registry.with_registry(build_mcp_registry_for_server(server).await?);
    }
    Ok(registry)
}
