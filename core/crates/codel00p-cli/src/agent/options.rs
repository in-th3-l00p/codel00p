//! CLI agent option types and flag parsing.

use super::*;

#[derive(Clone)]
pub(crate) struct AgentRunOptions {
    pub(crate) prompt: String,
    pub(crate) workspace: PathBuf,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) provider_policy_preset: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) max_iterations: Option<u32>,
    pub(crate) json_events: bool,
    pub(crate) stream_events: bool,
    pub(crate) stream: bool,
    pub(crate) tool_sets: Vec<AgentToolSet>,
    pub(crate) permission_mode: CliPermissionMode,
    pub(crate) remember_permissions: bool,
    pub(crate) mcp_servers: Vec<McpServerSpec>,
    /// When set, the turn is a messaging-gateway turn: privileged tools pause
    /// for a remote chat user's `/approve` decision instead of using the local
    /// CLI permission mode. See [`GatewayApprovalPolicy`].
    pub(crate) gateway_approval: Option<GatewayApproval>,
}

/// Routes a gateway turn's privileged-tool permissions through a remote chat
/// user's `/approve` / `/deny` decisions, backed by a file [`ApprovalStore`].
#[derive(Clone)]
pub(crate) struct GatewayApproval {
    pub(super) store: ApprovalStore,
    pub(super) conversation: String,
    /// A one-shot grant: the single tool the remote user just approved may run
    /// once without re-prompting. Any *other* privileged tool re-prompts.
    pub(super) granted_tool: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentToolSet {
    Read,
    Edit,
    Command,
    Git,
    Delegate,
    Learn,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliPermissionMode {
    Allow,
    Ask,
    Deny,
}

pub(super) enum AgentSessionMode {
    Fresh,
    Resume,
}

pub(super) fn parse_agent_run_options(
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<AgentRunOptions> {
    let Some(prompt) = args.first() else {
        return Err("missing agent prompt".to_string());
    };

    let mut options = parse_agent_flag_options(defaults, args, 1, "run")?;
    options.prompt = prompt.to_string();
    Ok(options)
}

pub(super) fn parse_agent_chat_options(
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<AgentRunOptions> {
    parse_agent_flag_options(defaults, args, 0, "chat")
}

fn parse_agent_flag_options(
    defaults: &AgentSettings,
    args: &[String],
    start: usize,
    context: &str,
) -> CliResult<AgentRunOptions> {
    let mut workspace = env::current_dir().map_err(|error| error.to_string())?;
    let mut provider = None;
    let mut model = None;
    let mut provider_policy_preset = None;
    let mut base_url = None;
    let mut session_id = None;
    let mut max_iterations = None;
    let mut json_events = false;
    let mut stream_events = false;
    let mut stream = None;
    let mut tool_sets = Vec::new();
    let mut permission_mode = None;
    let mut remember_permissions = None;
    let mut mcp_servers = Vec::new();
    let mut index = start;

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
            "--provider-policy-preset" => {
                provider_policy_preset =
                    Some(required_value(args, index, "--provider-policy-preset")?);
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
            "--stream" => {
                stream = Some(true);
                index += 1;
            }
            "--tool-set" => {
                let value = required_value(args, index, "--tool-set")?;
                tool_sets.push(parse_agent_tool_set(&value)?);
                index += 2;
            }
            "--permission-mode" => {
                let value = required_value(args, index, "--permission-mode")?;
                permission_mode = Some(parse_permission_mode(&value)?);
                index += 2;
            }
            "--remember-permissions" => {
                remember_permissions = Some(true);
                index += 1;
            }
            "--mcp-server" => {
                let value = required_value(args, index, "--mcp-server")?;
                mcp_servers.push(parse_mcp_server(&value)?);
                index += 2;
            }
            flag => return Err(format!("unknown agent {context} option: {flag}")),
        }
    }

    let provider = provider
        .or_else(|| defaults.provider.clone())
        .ok_or_else(|| {
            "no provider configured — run `codel00p providers use <id>` or pass --provider"
                .to_string()
        })?;
    let model = model.or_else(|| defaults.model.clone()).ok_or_else(|| {
        "no model configured — run `codel00p providers use <id> --model <model>` or pass --model"
            .to_string()
    })?;
    let provider_policy_preset =
        provider_policy_preset.or_else(|| defaults.provider_policy_preset.clone());
    let base_url = base_url.or_else(|| defaults.base_url.clone());
    let max_iterations = max_iterations.or(defaults.max_iterations);
    let permission_mode = match permission_mode {
        Some(mode) => mode,
        None => match &defaults.permission_mode {
            Some(value) => parse_permission_mode(value)?,
            None => CliPermissionMode::Allow,
        },
    };
    let tool_sets = if tool_sets.is_empty() {
        match &defaults.tool_sets {
            Some(values) => values
                .iter()
                .map(|value| parse_agent_tool_set(value))
                .collect::<CliResult<Vec<_>>>()?,
            None => Vec::new(),
        }
    } else {
        tool_sets
    };
    let stream = stream.or(defaults.stream).unwrap_or(false);
    let remember_permissions = remember_permissions
        .or(defaults.remember_permissions)
        .unwrap_or(false);

    Ok(AgentRunOptions {
        prompt: String::new(),
        workspace,
        provider,
        model,
        provider_policy_preset,
        base_url,
        session_id,
        max_iterations,
        json_events,
        stream_events,
        stream,
        tool_sets,
        permission_mode,
        remember_permissions,
        mcp_servers,
        gateway_approval: None,
    })
}

pub(super) fn parse_agent_resume_options(
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<AgentRunOptions> {
    if args.len() < 2 {
        return Err("usage: agent resume <session-id> <prompt>".to_string());
    }

    let session_id = args[0].clone();
    let mut options = parse_agent_run_options(defaults, &args[1..])?;
    options.session_id = Some(session_id);
    Ok(options)
}

pub(super) fn parse_agent_tool_set(value: &str) -> CliResult<AgentToolSet> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" | "read-only" | "readonly" => Ok(AgentToolSet::Read),
        "edit" | "editing" | "write" => Ok(AgentToolSet::Edit),
        "command" | "commands" | "shell" => Ok(AgentToolSet::Command),
        "git" => Ok(AgentToolSet::Git),
        "delegate" | "delegation" => Ok(AgentToolSet::Delegate),
        "learn" | "learning" => Ok(AgentToolSet::Learn),
        "all" => Ok(AgentToolSet::All),
        _ => Err(format!("unknown tool set: {value}")),
    }
}

pub(super) fn parse_permission_mode(value: &str) -> CliResult<CliPermissionMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" | "allowed" => Ok(CliPermissionMode::Allow),
        "ask" | "prompt" | "interactive" => Ok(CliPermissionMode::Ask),
        "deny" | "denied" => Ok(CliPermissionMode::Deny),
        _ => Err(format!("unknown permission mode: {value}")),
    }
}
