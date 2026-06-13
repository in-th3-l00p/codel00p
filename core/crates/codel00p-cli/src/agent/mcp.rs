//! MCP configuration parsing, diagnostics, and tool registry wiring for agent runs.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct McpServerSpec {
    pub(super) server_id: String,
    pub(super) command: Option<PathBuf>,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) url: Option<String>,
    pub(super) headers: Vec<(String, String)>,
    pub(super) bearer_token_env: Option<String>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) permission_scope: Option<PermissionScope>,
    pub(super) tool_scopes: HashMap<String, PermissionScope>,
}

pub(super) fn agent_mcp(_config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing agent mcp command".to_string());
    };
    match command.as_str() {
        "list" => agent_mcp_list(rest),
        "doctor" => agent_mcp_doctor(rest),
        _ => Err(format!("unknown agent mcp command: {command}")),
    }
}

pub(super) fn agent_mcp_list(args: &[String]) -> CliResult<String> {
    let options = parse_agent_mcp_list_options(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut servers = load_mcp_servers_from_workspace(&options.workspace)?;
        servers.extend(options.mcp_servers);
        let mut lines = Vec::new();
        for server in servers {
            for tool in list_mcp_tools_for_server(&server)
                .await
                .map_err(|error| error.to_string())?
            {
                lines.push(format!(
                    "{}\t{}",
                    tool.harness_tool_name(),
                    tool.description()
                ));
            }
        }
        lines.sort();
        Ok(if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        })
    })
}

pub(super) fn agent_mcp_doctor(args: &[String]) -> CliResult<String> {
    let options = parse_agent_mcp_list_options(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut servers = load_mcp_servers_from_workspace(&options.workspace)?;
        servers.extend(options.mcp_servers);
        let mut lines = Vec::new();
        for server in servers {
            lines.push(diagnose_mcp_server(&server).await);
        }
        lines.sort();
        Ok(if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        })
    })
}

pub(super) struct AgentMcpListOptions {
    pub(super) workspace: PathBuf,
    pub(super) mcp_servers: Vec<McpServerSpec>,
}

fn parse_agent_mcp_list_options(args: &[String]) -> CliResult<AgentMcpListOptions> {
    let mut workspace = env::current_dir().map_err(|error| error.to_string())?;
    let mut mcp_servers = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                workspace = PathBuf::from(required_value(args, index, "--workspace")?);
                index += 2;
            }
            "--mcp-server" => {
                let value = required_value(args, index, "--mcp-server")?;
                mcp_servers.push(parse_mcp_server(&value)?);
                index += 2;
            }
            flag => return Err(format!("unknown agent mcp list option: {flag}")),
        }
    }
    Ok(AgentMcpListOptions {
        workspace,
        mcp_servers,
    })
}

pub(super) fn parse_mcp_server(value: &str) -> CliResult<McpServerSpec> {
    let (server_id, command_spec) = value
        .split_once('=')
        .ok_or_else(|| "invalid --mcp-server, expected <id>=<command>".to_string())?;
    let server_id = server_id.trim();
    let command_spec = command_spec.trim();
    if server_id.is_empty() {
        return Err("invalid --mcp-server, server id is empty".to_string());
    }
    if command_spec.is_empty() {
        return Err("invalid --mcp-server, command is empty".to_string());
    }
    let mut tokens = split_command_spec(command_spec)?;
    let mut env = Vec::new();
    while let Some((key, value)) = tokens
        .first()
        .and_then(|token| parse_env_assignment_token(token))
    {
        env.push((key, value));
        tokens.remove(0);
    }
    let Some(command) = tokens.first() else {
        return Err("invalid --mcp-server, command is empty".to_string());
    };
    Ok(McpServerSpec {
        server_id: server_id.to_string(),
        command: Some(PathBuf::from(command)),
        args: tokens[1..].to_vec(),
        env,
        url: None,
        headers: Vec::new(),
        bearer_token_env: None,
        timeout_ms: None,
        permission_scope: None,
        tool_scopes: HashMap::new(),
    })
}

pub(super) fn load_mcp_servers_from_workspace(workspace: &Path) -> CliResult<Vec<McpServerSpec>> {
    let config_path = workspace.join(".codel00p/mcp.json");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let config = fs::read_to_string(&config_path)
        .map_err(|error| format!("failed to read {}: {error}", config_path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&config)
        .map_err(|error| format!("failed to parse {}: {error}", config_path.display()))?;
    let Some(servers) = value.get("servers").and_then(serde_json::Value::as_object) else {
        return Ok(Vec::new());
    };

    let mut specs = Vec::new();
    for (server_id, server) in servers {
        let command = server
            .get("command")
            .and_then(serde_json::Value::as_str)
            .map(|command| {
                let mut command = PathBuf::from(command);
                if command.is_relative() {
                    command = workspace.join(command);
                }
                command
            });
        let url = server
            .get("url")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        match (&command, &url) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "mcp server `{server_id}` must not define both command and url"
                ));
            }
            (None, None) => {
                return Err(format!(
                    "mcp server `{server_id}` is missing command or url"
                ));
            }
            _ => {}
        }
        let args = server
            .get("args")
            .and_then(serde_json::Value::as_array)
            .map(|args| {
                args.iter()
                    .map(|arg| {
                        arg.as_str()
                            .map(ToString::to_string)
                            .ok_or_else(|| format!("mcp server `{server_id}` has a non-string arg"))
                    })
                    .collect::<CliResult<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let env = server
            .get("env")
            .and_then(serde_json::Value::as_object)
            .map(|env| {
                env.iter()
                    .map(|(key, value)| {
                        value
                            .as_str()
                            .map(|value| (key.clone(), value.to_string()))
                            .ok_or_else(|| {
                                format!("mcp server `{server_id}` env `{key}` must be a string")
                            })
                    })
                    .collect::<CliResult<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let headers = server
            .get("headers")
            .and_then(serde_json::Value::as_object)
            .map(|headers| {
                headers
                    .iter()
                    .map(|(key, value)| {
                        value
                            .as_str()
                            .map(|value| (key.clone(), value.to_string()))
                            .ok_or_else(|| {
                                format!("mcp server `{server_id}` header `{key}` must be a string")
                            })
                    })
                    .collect::<CliResult<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let bearer_token_env = server
            .get("bearerTokenEnv")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        let timeout_ms = server.get("timeoutMs").and_then(serde_json::Value::as_u64);
        let permission_scope = server
            .get("permissionScope")
            .and_then(serde_json::Value::as_str)
            .map(parse_permission_scope)
            .transpose()
            .map_err(|error| format!("mcp server `{server_id}` {error}"))?;
        let tool_scopes = server
            .get("toolScopes")
            .and_then(serde_json::Value::as_object)
            .map(|tool_scopes| {
                tool_scopes
                    .iter()
                    .map(|(tool_name, value)| {
                        let scope = value.as_str().ok_or_else(|| {
                            format!(
                                "mcp server `{server_id}` tool scope `{tool_name}` must be a string"
                            )
                        })?;
                        Ok((tool_name.clone(), parse_permission_scope(scope)?))
                    })
                    .collect::<CliResult<HashMap<_, _>>>()
            })
            .transpose()?
            .unwrap_or_default();

        specs.push(McpServerSpec {
            server_id: server_id.clone(),
            command,
            args,
            env,
            url,
            headers,
            bearer_token_env,
            timeout_ms,
            permission_scope,
            tool_scopes,
        });
    }
    Ok(specs)
}

pub(super) fn parse_permission_scope(value: &str) -> CliResult<PermissionScope> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read_only" | "readonly" | "read-only" => Ok(PermissionScope::ReadOnly),
        "workspace_write" | "workspace-write" | "write" => Ok(PermissionScope::WorkspaceWrite),
        "shell" | "command" => Ok(PermissionScope::Shell),
        "external_connector" | "external-connector" | "external" => {
            Ok(PermissionScope::ExternalConnector)
        }
        _ => Err(format!("has unknown permission scope: {value}")),
    }
}

pub(super) fn parse_env_assignment_token(token: &str) -> Option<(String, String)> {
    let (key, value) = token.split_once('=')?;
    if key.is_empty()
        || key
            .chars()
            .any(|ch| !(ch == '_' || ch.is_ascii_alphanumeric()))
        || key.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

pub(super) fn split_command_spec(value: &str) -> CliResult<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, '\'') => quote = Some('\''),
            (None, '"') => quote = Some('"'),
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            (_, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                } else {
                    current.push('\\');
                }
            }
            _ => current.push(ch),
        }
    }
    if let Some(quote) = quote {
        return Err(format!("invalid --mcp-server, unterminated {quote} quote"));
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

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

pub(super) async fn diagnose_mcp_server_result(
    server: &McpServerSpec,
) -> CliResult<(usize, usize, usize)> {
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

pub(super) async fn build_mcp_registry_for_server(
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

fn stdio_command_from_spec(server: &McpServerSpec) -> CliResult<StdioServerCommand> {
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

fn http_client_from_spec(server: &McpServerSpec) -> CliResult<McpHttpClient> {
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
