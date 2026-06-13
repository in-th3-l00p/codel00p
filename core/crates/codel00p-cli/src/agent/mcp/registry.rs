use super::{
    clients::{http_client_from_spec, stdio_command_from_spec},
    spec::McpServerSpec,
    *,
};

pub(crate) async fn build_mcp_registry_for_server(
    server: &McpServerSpec,
) -> CliResult<ToolRegistry> {
    if server.url.is_some() {
        let mut client = http_client_from_spec(server)?;
        client
            .initialize()
            .await
            .map_err(|error| error.to_string())?;
        let client = Arc::new(tokio::sync::Mutex::new(client));
        let descriptors = client
            .list_tools()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(register_mcp_descriptors(server, client, descriptors));
    }

    let command = stdio_command_from_spec(server)?;
    let mut client = McpStdioClient::spawn(command)
        .await
        .map_err(|error| error.to_string())?;
    client
        .initialize()
        .await
        .map_err(|error| error.to_string())?;
    let client = Arc::new(tokio::sync::Mutex::new(client));
    let descriptors = client
        .list_tools()
        .await
        .map_err(|error| error.to_string())?;
    Ok(register_mcp_descriptors(server, client, descriptors))
}

fn register_mcp_descriptors<T>(
    server: &McpServerSpec,
    client: T,
    descriptors: Vec<McpToolDescriptor>,
) -> ToolRegistry
where
    T: McpClient + Clone + 'static,
{
    let mut mcp_registry = ToolRegistry::new();
    for descriptor in descriptors {
        let descriptor = match mcp_permission_scope_for_tool(server, descriptor.tool_name()) {
            Some(scope) => descriptor.with_permission_scope(scope),
            None => descriptor,
        };
        mcp_registry = mcp_registry.with_tool(McpTool::new(descriptor, client.clone()));
    }
    mcp_registry
}

fn mcp_permission_scope_for_tool(
    server: &McpServerSpec,
    tool_name: &str,
) -> Option<PermissionScope> {
    server
        .tool_scopes
        .get(tool_name)
        .copied()
        .or(server.permission_scope)
}
