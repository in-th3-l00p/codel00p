use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct McpServerSpec {
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

pub(crate) fn parse_mcp_server(value: &str) -> CliResult<McpServerSpec> {
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

pub(crate) fn load_mcp_servers_from_workspace(workspace: &Path) -> CliResult<Vec<McpServerSpec>> {
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

fn parse_permission_scope(value: &str) -> CliResult<PermissionScope> {
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

fn parse_env_assignment_token(token: &str) -> Option<(String, String)> {
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

fn split_command_spec(value: &str) -> CliResult<Vec<String>> {
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
