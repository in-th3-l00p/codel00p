use super::{
    clients::{http_client_from_spec, stdio_command_from_spec},
    spec::McpServerSpec,
    *,
};

pub(super) async fn list_mcp_tools_for_server(
    server: &McpServerSpec,
) -> CliResult<Vec<McpToolDescriptor>> {
    if server.url.is_some() {
        let mut client = http_client_from_spec(server)?;
        client
            .initialize()
            .await
            .map_err(|error| error.to_string())?;
        return client.list_tools().await.map_err(|error| error.to_string());
    }

    let command = stdio_command_from_spec(server)?;
    let mut client = McpStdioClient::spawn(command)
        .await
        .map_err(|error| error.to_string())?;
    client
        .initialize()
        .await
        .map_err(|error| error.to_string())?;
    let tools = client
        .list_tools()
        .await
        .map_err(|error| error.to_string())?;
    let _ = client.shutdown().await;
    Ok(tools)
}

pub(super) async fn diagnose_mcp_server(server: &McpServerSpec) -> String {
    match diagnose_mcp_server_result(server).await {
        Ok((tools, resources, prompts)) => format!(
            "{}\tok\t{}\ttools={tools}\tresources={resources}\tprompts={prompts}\t{}",
            server.server_id,
            mcp_server_transport(server),
            redacted_mcp_server_details(server)
        ),
        Err(error) => format!(
            "{}\terror\t{}\ttools=0\tresources=0\tprompts=0\t{}\tmessage={}",
            server.server_id,
            mcp_server_transport(server),
            redacted_mcp_server_details(server),
            sanitize_diagnostic_field(&error)
        ),
    }
}

async fn diagnose_mcp_server_result(server: &McpServerSpec) -> CliResult<(usize, usize, usize)> {
    if server.url.is_some() {
        let mut client = http_client_from_spec(server)?;
        let initialization = client
            .initialize()
            .await
            .map_err(|error| error.to_string())?;
        let tools = if mcp_capability_advertised(initialization.capabilities(), "tools") {
            client
                .list_tools()
                .await
                .map_err(|error| error.to_string())?
                .len()
        } else {
            0
        };
        let resources = if mcp_capability_advertised(initialization.capabilities(), "resources") {
            client
                .list_resources()
                .await
                .map_err(|error| error.to_string())?
                .len()
        } else {
            0
        };
        let prompts = if mcp_capability_advertised(initialization.capabilities(), "prompts") {
            client
                .list_prompts()
                .await
                .map_err(|error| error.to_string())?
                .len()
        } else {
            0
        };
        return Ok((tools, resources, prompts));
    }

    let command = stdio_command_from_spec(server)?;
    let mut client = McpStdioClient::spawn(command)
        .await
        .map_err(|error| error.to_string())?;
    let initialization = client
        .initialize()
        .await
        .map_err(|error| error.to_string())?;
    let tools = if mcp_capability_advertised(initialization.capabilities(), "tools") {
        client
            .list_tools()
            .await
            .map_err(|error| error.to_string())?
            .len()
    } else {
        0
    };
    let resources = if mcp_capability_advertised(initialization.capabilities(), "resources") {
        client
            .list_resources()
            .await
            .map_err(|error| error.to_string())?
            .len()
    } else {
        0
    };
    let prompts = if mcp_capability_advertised(initialization.capabilities(), "prompts") {
        client
            .list_prompts()
            .await
            .map_err(|error| error.to_string())?
            .len()
    } else {
        0
    };
    let _ = client.shutdown().await;
    Ok((tools, resources, prompts))
}

fn mcp_capability_advertised(capabilities: &serde_json::Value, name: &str) -> bool {
    capabilities.get(name).is_some()
}

fn mcp_server_transport(server: &McpServerSpec) -> &'static str {
    if server.url.is_some() {
        "http"
    } else {
        "stdio"
    }
}

fn redacted_mcp_server_details(server: &McpServerSpec) -> String {
    let mut parts = Vec::new();
    if let Some(command) = &server.command {
        parts.push(format!(
            "command={}",
            sanitize_diagnostic_field(&command.display().to_string())
        ));
    }
    if let Some(url) = &server.url {
        parts.push(format!("url={}", sanitize_diagnostic_field(url)));
    }
    if !server.env.is_empty() {
        let mut env = server
            .env
            .iter()
            .map(|(key, _value)| format!("{key}:<redacted>"))
            .collect::<Vec<_>>();
        env.sort();
        parts.push(format!("env={}", env.join(",")));
    }
    if !server.headers.is_empty() {
        let mut headers = server
            .headers
            .iter()
            .map(|(key, _value)| format!("{key}:<redacted>"))
            .collect::<Vec<_>>();
        headers.sort();
        parts.push(format!("headers={}", headers.join(",")));
    }
    if let Some(env_var) = &server.bearer_token_env {
        let status = if env::var(env_var).is_ok() {
            "set"
        } else {
            "missing"
        };
        parts.push(format!("bearer_token_env={env_var}:{status}"));
    }
    if let Some(timeout_ms) = server.timeout_ms {
        parts.push(format!("timeout_ms={timeout_ms}"));
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join("\t")
    }
}

fn sanitize_diagnostic_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch == '\t' || ch == '\n' || ch == '\r' {
                ' '
            } else {
                ch
            }
        })
        .collect()
}
