use super::{spec::McpServerSpec, *};

pub(super) fn stdio_command_from_spec(server: &McpServerSpec) -> CliResult<StdioServerCommand> {
    let command_path = server
        .command
        .clone()
        .ok_or_else(|| format!("mcp server `{}` is missing command", server.server_id))?;
    let command = StdioServerCommand::new(server.server_id.clone(), command_path, &server.args)
        .with_envs(
            server
                .env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
    Ok(if let Some(timeout_ms) = server.timeout_ms {
        command.with_request_timeout(Duration::from_millis(timeout_ms))
    } else {
        command
    })
}

pub(super) fn http_client_from_spec(server: &McpServerSpec) -> CliResult<McpHttpClient> {
    let url = server
        .url
        .clone()
        .ok_or_else(|| format!("mcp server `{}` is missing url", server.server_id))?;
    let mut endpoint = HttpServerEndpoint::new(server.server_id.clone(), url);
    for (key, value) in &server.headers {
        endpoint = endpoint.with_header(key, value);
    }
    if let Some(env_var) = &server.bearer_token_env {
        let token = env::var(env_var).map_err(|_| {
            format!(
                "mcp server `{}` missing bearer token env `{env_var}`",
                server.server_id
            )
        })?;
        endpoint = endpoint.with_bearer_token(token);
    }
    if let Some(timeout_ms) = server.timeout_ms {
        endpoint = endpoint.with_request_timeout(Duration::from_millis(timeout_ms));
    }
    McpHttpClient::connect(endpoint).map_err(|error| error.to_string())
}
