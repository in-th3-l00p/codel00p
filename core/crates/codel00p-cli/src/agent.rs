use std::{
    collections::HashMap,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use codel00p_harness::{
    AgentEventSink, AgentHarness, ExplicitTurnMemoryExtractor, HarnessError, HarnessEvent,
    MemoryCandidateSink, MemoryCandidateSinkOutcome, PermissionDecision, PermissionMode,
    PermissionPolicy, PermissionRequest, PermissionScope, ProjectMemoryContext, ProjectMemoryItem,
    ProjectMemoryProvider, ProjectMemoryRequest, ProviderModelClient, ToolRegistry, UserMessage,
    Workspace,
};
use codel00p_mcp::{
    HttpServerEndpoint, McpClient, McpHttpClient, McpStdioClient, McpTool, McpToolDescriptor,
    StdioServerCommand,
};
use codel00p_memory::{MemoryCandidateInput, MemoryError, MemoryQuery, MemoryRepository};
use codel00p_protocol::AgentEvent;
use codel00p_session::{SessionMetadata, SessionRecord, SessionStore, SessionStoreError};

use crate::{
    config::{
        CliConfig, CliResult, open_memory_store, open_session_store, parse_session_id,
        required_value,
    },
    connector_permissions::{
        ConnectorPermissionDecision, ConnectorPermissionStatus, is_rememberable_permission,
        load_decision, remember_decision,
    },
    providers::build_provider_client,
};

struct AgentRunOptions {
    prompt: String,
    workspace: PathBuf,
    provider: String,
    model: String,
    base_url: Option<String>,
    session_id: Option<String>,
    max_iterations: Option<u32>,
    json_events: bool,
    stream_events: bool,
    tool_sets: Vec<AgentToolSet>,
    permission_mode: CliPermissionMode,
    remember_permissions: bool,
    mcp_servers: Vec<McpServerSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentToolSet {
    Read,
    Edit,
    Command,
    Git,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CliPermissionMode {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct McpServerSpec {
    server_id: String,
    command: Option<PathBuf>,
    args: Vec<String>,
    env: Vec<(String, String)>,
    url: Option<String>,
    headers: Vec<(String, String)>,
    bearer_token_env: Option<String>,
    timeout_ms: Option<u64>,
    permission_scope: Option<PermissionScope>,
    tool_scopes: HashMap<String, PermissionScope>,
}

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing agent command".to_string());
    };

    match command.as_str() {
        "run" => agent_run(config, rest),
        "resume" => agent_resume(config, rest),
        "mcp" => agent_mcp(config, rest),
        _ => Err(format!("unknown agent command: {command}")),
    }
}

fn agent_run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let options = parse_agent_run_options(args)?;
    run_agent_turn(config, options, AgentSessionMode::Fresh)
}

fn agent_resume(config: CliConfig, args: &[String]) -> CliResult<String> {
    let options = parse_agent_resume_options(args)?;
    run_agent_turn(config, options, AgentSessionMode::Resume)
}

fn agent_mcp(_config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing agent mcp command".to_string());
    };
    match command.as_str() {
        "list" => agent_mcp_list(rest),
        "doctor" => agent_mcp_doctor(rest),
        _ => Err(format!("unknown agent mcp command: {command}")),
    }
}

enum AgentSessionMode {
    Fresh,
    Resume,
}

fn agent_mcp_list(args: &[String]) -> CliResult<String> {
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

fn agent_mcp_doctor(args: &[String]) -> CliResult<String> {
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

struct AgentMcpListOptions {
    workspace: PathBuf,
    mcp_servers: Vec<McpServerSpec>,
}

fn run_agent_turn(
    config: CliConfig,
    options: AgentRunOptions,
    session_mode: AgentSessionMode,
) -> CliResult<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let provider_client = build_provider_client(&options.provider)?;
        let model_client =
            ProviderModelClient::new(provider_client, &options.provider, &options.model);
        let model_client = if let Some(base_url) = &options.base_url {
            model_client.with_base_url(base_url)
        } else {
            model_client
        };

        let workspace = Workspace::new(&options.workspace).map_err(|error| error.to_string())?;
        let memory_provider = CliProjectMemoryProvider::new(config.clone()).with_limit(8);
        let memory_sink = CliMemoryCandidateSink::new(config.clone());
        let memory_extractor = ExplicitTurnMemoryExtractor::new(config.project.clone())
            .with_tag("agent")
            .with_tag("cli");
        let mut mcp_servers = load_mcp_servers_from_workspace(&options.workspace)?;
        mcp_servers.extend(options.mcp_servers.clone());

        let mut builder = AgentHarness::builder()
            .model_client(model_client)
            .workspace(workspace)
            .tools(build_tool_registry(&options.tool_sets, &mcp_servers).await?)
            .permission_policy(CliPermissionPolicy::new(
                config.clone(),
                options.permission_mode,
                options.remember_permissions,
            ))
            .project_memory_provider(memory_provider)
            .turn_memory_extractor(memory_extractor)
            .memory_candidate_sink(memory_sink);
        if options.stream_events {
            builder = builder.event_sink(StdoutJsonEventSink);
        }
        if let Some(max_iterations) = options.max_iterations {
            builder = builder.max_iterations(max_iterations);
        }

        let (session_state, previous_message_count) =
            prepare_session_state(&config, &options, session_mode)?;

        let outcome = builder
            .build()
            .map_err(|error| error.to_string())?
            .run_turn_with_state(session_state, UserMessage::new(options.prompt))
            .await
            .map_err(|error| error.to_string())?;

        let mut output = String::new();
        if let Some(message) = &outcome.assistant_message {
            output.push_str(message);
            output.push('\n');
        }
        if options.json_events {
            for event in &outcome.events {
                output.push_str(&serde_json::to_string(&event).map_err(|error| error.to_string())?);
                output.push('\n');
            }
        }
        persist_turn_outcome(
            &config,
            &outcome.session_state,
            &outcome.events,
            previous_message_count,
        )?;

        Ok(output)
    })
}

fn prepare_session_state(
    config: &CliConfig,
    options: &AgentRunOptions,
    session_mode: AgentSessionMode,
) -> CliResult<(codel00p_harness::SessionState, usize)> {
    match session_mode {
        AgentSessionMode::Fresh => {
            let session_id = options
                .session_id
                .as_deref()
                .map(parse_session_id)
                .transpose()?
                .unwrap_or_default();
            Ok((codel00p_harness::SessionState::new(session_id), 0))
        }
        AgentSessionMode::Resume => {
            let session_id = options
                .session_id
                .as_deref()
                .ok_or_else(|| "missing resume session id".to_string())
                .and_then(parse_session_id)?;
            let session_state = replay_session_messages(config, session_id)?;
            let previous_message_count = session_state.messages().len();
            Ok((session_state, previous_message_count))
        }
    }
}

fn parse_agent_run_options(args: &[String]) -> CliResult<AgentRunOptions> {
    let Some(prompt) = args.first() else {
        return Err("missing agent prompt".to_string());
    };

    let mut workspace = env::current_dir().map_err(|error| error.to_string())?;
    let mut provider = None;
    let mut model = None;
    let mut base_url = None;
    let mut session_id = None;
    let mut max_iterations = None;
    let mut json_events = false;
    let mut stream_events = false;
    let mut tool_sets = Vec::new();
    let mut permission_mode = CliPermissionMode::Allow;
    let mut remember_permissions = false;
    let mut mcp_servers = Vec::new();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                workspace = PathBuf::from(required_value(args, index, "--workspace")?);
                index += 2;
            }
            "--provider" => {
                provider = Some(required_value(args, index, "--provider")?);
                index += 2;
            }
            "--model" => {
                model = Some(required_value(args, index, "--model")?);
                index += 2;
            }
            "--base-url" => {
                base_url = Some(required_value(args, index, "--base-url")?);
                index += 2;
            }
            "--session-id" => {
                session_id = Some(required_value(args, index, "--session-id")?);
                index += 2;
            }
            "--max-iterations" => {
                let value = required_value(args, index, "--max-iterations")?
                    .parse::<u32>()
                    .map_err(|_| "invalid --max-iterations".to_string())?;
                max_iterations = Some(value);
                index += 2;
            }
            "--json-events" => {
                json_events = true;
                index += 1;
            }
            "--stream-events" => {
                stream_events = true;
                index += 1;
            }
            "--tool-set" => {
                let value = required_value(args, index, "--tool-set")?;
                tool_sets.push(parse_agent_tool_set(&value)?);
                index += 2;
            }
            "--permission-mode" => {
                let value = required_value(args, index, "--permission-mode")?;
                permission_mode = parse_permission_mode(&value)?;
                index += 2;
            }
            "--remember-permissions" => {
                remember_permissions = true;
                index += 1;
            }
            "--mcp-server" => {
                let value = required_value(args, index, "--mcp-server")?;
                mcp_servers.push(parse_mcp_server(&value)?);
                index += 2;
            }
            flag => return Err(format!("unknown agent run option: {flag}")),
        }
    }

    Ok(AgentRunOptions {
        prompt: prompt.to_string(),
        workspace,
        provider: provider.ok_or_else(|| "missing required --provider".to_string())?,
        model: model.ok_or_else(|| "missing required --model".to_string())?,
        base_url,
        session_id,
        max_iterations,
        json_events,
        stream_events,
        tool_sets,
        permission_mode,
        remember_permissions,
        mcp_servers,
    })
}

fn parse_agent_resume_options(args: &[String]) -> CliResult<AgentRunOptions> {
    if args.len() < 2 {
        return Err("usage: agent resume <session-id> <prompt>".to_string());
    }

    let session_id = args[0].clone();
    let mut options = parse_agent_run_options(&args[1..])?;
    options.session_id = Some(session_id);
    Ok(options)
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

fn parse_agent_tool_set(value: &str) -> CliResult<AgentToolSet> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" | "read-only" | "readonly" => Ok(AgentToolSet::Read),
        "edit" | "editing" | "write" => Ok(AgentToolSet::Edit),
        "command" | "commands" | "shell" => Ok(AgentToolSet::Command),
        "git" => Ok(AgentToolSet::Git),
        "all" => Ok(AgentToolSet::All),
        _ => Err(format!("unknown tool set: {value}")),
    }
}

fn parse_permission_mode(value: &str) -> CliResult<CliPermissionMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" | "allowed" => Ok(CliPermissionMode::Allow),
        "ask" | "prompt" | "interactive" => Ok(CliPermissionMode::Ask),
        "deny" | "denied" => Ok(CliPermissionMode::Deny),
        _ => Err(format!("unknown permission mode: {value}")),
    }
}

fn parse_mcp_server(value: &str) -> CliResult<McpServerSpec> {
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

fn load_mcp_servers_from_workspace(workspace: &Path) -> CliResult<Vec<McpServerSpec>> {
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

async fn build_tool_registry(
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
            AgentToolSet::All => registry
                .with_registry(ToolRegistry::editing_defaults())
                .with_registry(ToolRegistry::command_defaults())
                .with_registry(ToolRegistry::git_defaults()),
        };
    }
    for server in mcp_servers {
        registry = registry.with_registry(build_mcp_registry_for_server(server).await?);
    }
    Ok(registry)
}

async fn list_mcp_tools_for_server(server: &McpServerSpec) -> CliResult<Vec<McpToolDescriptor>> {
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

async fn diagnose_mcp_server(server: &McpServerSpec) -> String {
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

async fn build_mcp_registry_for_server(server: &McpServerSpec) -> CliResult<ToolRegistry> {
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

struct CliPermissionPolicy {
    config: CliConfig,
    mode: CliPermissionMode,
    remember_permissions: bool,
    prompt_lock: Arc<Mutex<()>>,
}

impl CliPermissionPolicy {
    fn new(config: CliConfig, mode: CliPermissionMode, remember_permissions: bool) -> Self {
        Self {
            config,
            mode,
            remember_permissions,
            prompt_lock: Arc::new(Mutex::new(())),
        }
    }
}

#[async_trait]
impl PermissionPolicy for CliPermissionPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        match self.mode {
            CliPermissionMode::Allow => Ok(PermissionDecision::allow(
                request.id(),
                PermissionMode::Allow,
            )),
            CliPermissionMode::Ask => {
                if let Some(decision) = self.remembered_decision(&request)? {
                    return Ok(decision);
                }
                let decision = self.decide_with_prompt(&request)?;
                self.persist_decision_if_needed(&request, &decision)?;
                Ok(decision)
            }
            CliPermissionMode::Deny => Ok(PermissionDecision::deny(
                request.id(),
                PermissionMode::Deny,
                format!("{} denied by CLI permission mode", request.tool_name()),
            )),
        }
    }
}

impl CliPermissionPolicy {
    fn remembered_decision(
        &self,
        request: &PermissionRequest,
    ) -> Result<Option<PermissionDecision>, HarnessError> {
        if !self.should_remember(request) {
            return Ok(None);
        }
        let decision =
            load_decision(&self.config, request.tool_name(), request.scope()).map_err(|error| {
                HarnessError::ToolFailed {
                    name: request.tool_name().to_string(),
                    message: format!("failed to read remembered permission: {error}"),
                }
            })?;
        Ok(match decision.map(|decision| decision.status) {
            Some(ConnectorPermissionStatus::Allow) => {
                Some(PermissionDecision::allow(request.id(), PermissionMode::Ask))
            }
            Some(ConnectorPermissionStatus::Deny) => Some(PermissionDecision::deny(
                request.id(),
                PermissionMode::Ask,
                format!(
                    "{} denied by remembered connector policy",
                    request.tool_name()
                ),
            )),
            None => None,
        })
    }

    fn persist_decision_if_needed(
        &self,
        request: &PermissionRequest,
        decision: &PermissionDecision,
    ) -> Result<(), HarnessError> {
        if !self.should_remember(request) {
            return Ok(());
        }
        let status = if decision.allows_execution() {
            ConnectorPermissionStatus::Allow
        } else {
            ConnectorPermissionStatus::Deny
        };
        remember_decision(
            &self.config,
            ConnectorPermissionDecision {
                tool_name: request.tool_name().to_string(),
                scope: request.scope(),
                status,
            },
        )
        .map_err(|error| HarnessError::ToolFailed {
            name: request.tool_name().to_string(),
            message: format!("failed to remember permission: {error}"),
        })?;
        Ok(())
    }

    fn should_remember(&self, request: &PermissionRequest) -> bool {
        self.remember_permissions
            && is_rememberable_permission(request.tool_name(), request.scope())
    }

    fn decide_with_prompt(
        &self,
        request: &PermissionRequest,
    ) -> Result<PermissionDecision, HarnessError> {
        let _prompt = self
            .prompt_lock
            .lock()
            .map_err(|_| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: "permission prompt lock was poisoned".to_string(),
            })?;

        let approved =
            prompt_for_permission(request).map_err(|error| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: format!("failed to read permission approval: {error}"),
            })?;

        if approved {
            Ok(PermissionDecision::allow(request.id(), PermissionMode::Ask))
        } else {
            Ok(PermissionDecision::deny(
                request.id(),
                PermissionMode::Ask,
                format!("{} rejected by CLI approval prompt", request.tool_name()),
            ))
        }
    }
}

fn prompt_for_permission(request: &PermissionRequest) -> io::Result<bool> {
    let mut stderr = io::stderr();
    write!(
        stderr,
        "Allow tool `{}` for {:?}? [y/N] ",
        request.tool_name(),
        request.scope()
    )?;
    stderr.flush()?;

    let mut answer = String::new();
    let bytes = io::stdin().read_line(&mut answer)?;
    if bytes == 0 {
        return Ok(false);
    }

    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

struct StdoutJsonEventSink;

#[async_trait]
impl AgentEventSink for StdoutJsonEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        if let Ok(encoded) = serde_json::to_string(event) {
            println!("{encoded}");
        }
    }
}

fn replay_session_messages(
    config: &CliConfig,
    session_id: codel00p_harness::SessionId,
) -> CliResult<codel00p_harness::SessionState> {
    let store = open_session_store(config)?;
    let records = store
        .replay(&session_id)
        .map_err(|error| error.to_string())?;
    let mut session_state = codel00p_harness::SessionState::new(session_id);

    for record in records {
        if let SessionRecord::Message(message) = record.record() {
            session_state.push_message(message.clone());
        }
    }

    Ok(session_state)
}

fn persist_turn_outcome(
    config: &CliConfig,
    session_state: &codel00p_harness::SessionState,
    events: &[AgentEvent],
    message_start_index: usize,
) -> CliResult<()> {
    let mut store = open_session_store(config)?;
    match store.create_session(SessionMetadata::new(
        session_state.session_id().clone(),
        "cli",
    )) {
        Ok(()) | Err(SessionStoreError::SessionAlreadyExists { .. }) => {}
        Err(error) => return Err(error.to_string()),
    }

    for message in &session_state.messages()[message_start_index..] {
        store
            .append_message(session_state.session_id(), message.clone())
            .map_err(|error| error.to_string())?;
    }
    for event in events {
        store
            .append_event(session_state.session_id(), event.clone())
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

struct CliProjectMemoryProvider {
    config: CliConfig,
    limit: Option<usize>,
}

impl CliProjectMemoryProvider {
    fn new(config: CliConfig) -> Self {
        Self {
            config,
            limit: None,
        }
    }

    fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

#[async_trait]
impl ProjectMemoryProvider for CliProjectMemoryProvider {
    async fn retrieve(
        &self,
        _request: ProjectMemoryRequest,
    ) -> Result<ProjectMemoryContext, codel00p_harness::HarnessError> {
        let store = open_memory_store(&self.config)
            .map_err(|message| codel00p_harness::HarnessError::InferenceFailed { message })?;
        let mut query = MemoryQuery::new(self.config.project.clone());
        if let Some(limit) = self.limit {
            query = query.with_limit(limit);
        }

        let items = store
            .retrieve(query)
            .map_err(|error| codel00p_harness::HarnessError::InferenceFailed {
                message: error.to_string(),
            })?
            .into_iter()
            .map(|memory| {
                ProjectMemoryItem::new(
                    memory.entry().id(),
                    memory.entry().kind(),
                    memory.entry().content(),
                    memory.entry().tags().to_vec(),
                    memory.reason(),
                )
            })
            .collect();

        Ok(ProjectMemoryContext::new(items))
    }
}

struct CliMemoryCandidateSink {
    config: CliConfig,
}

impl CliMemoryCandidateSink {
    fn new(config: CliConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl MemoryCandidateSink for CliMemoryCandidateSink {
    async fn persist(
        &self,
        candidates: Vec<MemoryCandidateInput>,
    ) -> Result<MemoryCandidateSinkOutcome, codel00p_harness::HarnessError> {
        let mut store = open_memory_store(&self.config)
            .map_err(|message| codel00p_harness::HarnessError::InferenceFailed { message })?;
        let mut created_ids = Vec::new();
        let mut duplicate_ids = Vec::new();

        for candidate in candidates {
            let id = candidate.id().to_string();
            match store.create_candidate(candidate) {
                Ok(_) => created_ids.push(id),
                Err(MemoryError::MemoryAlreadyExists { .. }) => duplicate_ids.push(id),
                Err(error) => {
                    return Err(codel00p_harness::HarnessError::InferenceFailed {
                        message: error.to_string(),
                    });
                }
            }
        }

        Ok(MemoryCandidateSinkOutcome::from_parts(
            created_ids,
            duplicate_ids,
        ))
    }
}
