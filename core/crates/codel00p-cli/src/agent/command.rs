//! Top-level agent subcommand dispatch and gateway entrypoints.

use super::*;

pub fn run(config: CliConfig, defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    // No subcommand opens the interactive chat — the primary UI.
    let Some((command, rest)) = args.split_first() else {
        return agent_chat(config, &defaults, &[]);
    };

    match command.as_str() {
        "run" => agent_run(config, &defaults, rest),
        "resume" => agent_resume(config, &defaults, rest),
        "continue" => agent_continue(config, &defaults, rest),
        "chat" => agent_chat(config, &defaults, rest),
        "mcp" => agent_mcp(config, rest),
        _ => Err(format!("unknown agent command: {command}")),
    }
}

fn agent_run(config: CliConfig, defaults: &AgentSettings, args: &[String]) -> CliResult<String> {
    let options = parse_agent_run_options(defaults, args)?;
    run_agent_turn(config, options, AgentSessionMode::Fresh)
}

/// Run a scheduled job as a fresh, restricted agent turn.
///
/// Provider and model come from the job, falling back to `agent.*` config.
/// Unattended runs use a read-only tool set, so a schedule can never silently
/// edit files or run shell commands until that is deliberately opted into. The
/// run is persisted as a normal session, so it is auditable.
pub(crate) fn run_scheduled_job(
    config: CliConfig,
    defaults: &AgentSettings,
    job: &CronJob,
) -> CliResult<String> {
    let provider = job
        .provider
        .clone()
        .or_else(|| defaults.provider.clone())
        .ok_or_else(|| {
            "no provider configured; set agent.provider or the job's provider".to_string()
        })?;
    let model = job
        .model
        .clone()
        .or_else(|| defaults.model.clone())
        .ok_or_else(|| "no model configured; set agent.model or the job's model".to_string())?;
    let workspace = match &job.workspace {
        Some(path) => PathBuf::from(path),
        None => env::current_dir().map_err(|error| error.to_string())?,
    };

    let options = AgentRunOptions {
        prompt: job.prompt.clone(),
        workspace,
        provider,
        model,
        provider_policy_preset: defaults.provider_policy_preset.clone(),
        base_url: defaults.base_url.clone(),
        session_id: None,
        max_iterations: defaults.max_iterations,
        json_events: false,
        stream_events: false,
        stream: false,
        // Restricted by default: an unattended run may only read.
        tool_sets: vec![AgentToolSet::Read],
        tool_choice: None,
        response_format: None,
        permission_mode: CliPermissionMode::Allow,
        remember_permissions: false,
        mcp_servers: Vec::new(),
        fallback_routes: resolve_configured_fallback_routes(defaults.fallbacks.as_ref())?,
        gateway_approval: None,
        // Scheduled/cron job: no operator present.
        unattended: true,
    };

    run_agent_turn(config, options, AgentSessionMode::Fresh)
}

/// The on-disk directory backing the gateway's pending-approval store.
///
/// Lives next to the conversation sessions (under `CODEL00P_HOME` in practice,
/// isolated per test) so a pending request survives a process restart and is
/// shared across the processes a deployment may run.
fn approval_store_dir(config: &CliConfig) -> PathBuf {
    config
        .memory_db
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
        .join("gateway-approvals")
}

/// Handle one inbound gateway message for a conversation.
///
/// Control commands are answered directly; ordinary text runs as an agent turn
/// against the conversation's durable session (derived from the conversation
/// id), so a chat thread is one continuous, resumable session. Privileged tools
/// (edit, shell) are gated: the turn pauses and asks the remote user, who
/// replies `/approve` or `/deny` — see [`GatewayApprovalPolicy`]. The `user` is
/// the platform sender — recorded by the adapter; full identity governance
/// (mapping to a codel00p org/role) is a later slice.
pub(crate) fn run_gateway_message(
    config: CliConfig,
    defaults: &AgentSettings,
    conversation: &str,
    user: &str,
    text: &str,
) -> CliResult<String> {
    let _ = user;
    match codel00p_gateway::parse_command(text) {
        GatewayCommand::Help => Ok(format!("{}\n", codel00p_gateway::help_text())),
        GatewayCommand::Stop => Ok("Nothing is currently running.\n".to_string()),
        GatewayCommand::Approve => gateway_resolve(config, defaults, conversation, true),
        GatewayCommand::Deny => gateway_resolve(config, defaults, conversation, false),
        GatewayCommand::Message(message) => {
            let store = ApprovalStore::new(approval_store_dir(&config));
            // A fresh message supersedes any stale pending request for the thread.
            let _ = store.resolve(conversation, false);
            gateway_turn(config, defaults, conversation, message, None)
        }
    }
}

/// Resolve the conversation's pending approval with the remote user's decision.
///
/// On `/deny`, the request is dropped. On `/approve`, the agent turn is resumed
/// with a one-shot grant for exactly the approved tool, so it can complete the
/// action the user just authorized (and re-prompt for anything further).
fn gateway_resolve(
    config: CliConfig,
    defaults: &AgentSettings,
    conversation: &str,
    approve: bool,
) -> CliResult<String> {
    let store = ApprovalStore::new(approval_store_dir(&config));
    let pending = store.pending(conversation);
    match store.resolve(conversation, approve) {
        ApprovalOutcome::Nothing => Ok("No permission request is pending.\n".to_string()),
        ApprovalOutcome::Denied => {
            let tool = pending
                .map(|p| p.tool)
                .unwrap_or_else(|| "request".to_string());
            Ok(format!("Denied the pending request to use `{tool}`.\n"))
        }
        ApprovalOutcome::Approved => {
            let granted = pending.map(|p| p.tool);
            gateway_turn(
                config,
                defaults,
                conversation,
                "The remote user approved your pending request. Carry it out and report the result."
                    .to_string(),
                granted,
            )
        }
    }
}

/// Run (or resume) the conversation's agent turn, gating privileged tools on the
/// remote user's approval. If the turn parks on a permission request, the reply
/// is replaced with a clear prompt telling the user how to approve or deny.
fn gateway_turn(
    config: CliConfig,
    defaults: &AgentSettings,
    conversation: &str,
    prompt: String,
    granted_tool: Option<String>,
) -> CliResult<String> {
    let provider = defaults
        .provider
        .clone()
        .ok_or_else(|| "no provider configured; set agent.provider".to_string())?;
    let model = defaults
        .model
        .clone()
        .ok_or_else(|| "no model configured; set agent.model".to_string())?;
    let session_id = codel00p_gateway::conversation_session_id(conversation);
    let parsed = parse_session_id(&session_id)?;
    let mode = if session_exists(&config, &parsed) {
        AgentSessionMode::Resume
    } else {
        AgentSessionMode::Fresh
    };
    let workspace = env::current_dir().map_err(|error| error.to_string())?;
    let store = ApprovalStore::new(approval_store_dir(&config));

    let options = AgentRunOptions {
        prompt,
        workspace,
        provider,
        model,
        provider_policy_preset: defaults.provider_policy_preset.clone(),
        base_url: defaults.base_url.clone(),
        session_id: Some(session_id),
        max_iterations: defaults.max_iterations,
        json_events: false,
        stream_events: false,
        stream: false,
        // Read freely; edit/shell are available but pause for the remote user's
        // `/approve` via the gateway approval policy below.
        tool_sets: vec![
            AgentToolSet::Read,
            AgentToolSet::Edit,
            AgentToolSet::Command,
        ],
        tool_choice: None,
        response_format: None,
        permission_mode: CliPermissionMode::Allow,
        remember_permissions: false,
        mcp_servers: Vec::new(),
        fallback_routes: resolve_configured_fallback_routes(defaults.fallbacks.as_ref())?,
        gateway_approval: Some(GatewayApproval {
            store: store.clone(),
            conversation: conversation.to_string(),
            granted_tool,
        }),
        // Messaging-gateway turn: driven by a remote chat user, no local operator.
        unattended: true,
    };
    let reply = run_agent_turn(config, options, mode)?;

    // If a privileged tool parked the turn, surface the request instead of the
    // agent's "I was denied" narration.
    Ok(match store.pending(conversation) {
        Some(pending) => format!(
            "\u{1f512} Approval needed to use `{}`.\n{}\nReply /approve to allow or /deny to reject.\n",
            pending.tool, pending.detail
        ),
        None => reply,
    })
}

fn session_exists(config: &CliConfig, session_id: &SessionId) -> bool {
    open_session_store(config)
        .ok()
        .and_then(|store| store.metadata(session_id).ok())
        .is_some()
}

fn agent_resume(config: CliConfig, defaults: &AgentSettings, args: &[String]) -> CliResult<String> {
    let options = parse_agent_resume_options(defaults, args)?;
    run_agent_turn(config, options, AgentSessionMode::Resume)
}

/// `agent continue <prompt>` — resume the most recently created session without
/// naming its id. Resolves the latest session, then runs the resume path.
fn agent_continue(
    config: CliConfig,
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<String> {
    let session_id = {
        let store = open_session_store(&config)?;
        let sessions = store.list_sessions().map_err(|error| error.to_string())?;
        latest_session_id(&sessions).ok_or_else(|| {
            "no saved sessions to continue; start one with `agent run` or `agent chat`".to_string()
        })?
    };
    let mut options = parse_agent_run_options(defaults, args)?;
    options.session_id = Some(session_id);
    run_agent_turn(config, options, AgentSessionMode::Resume)
}

/// Continue an existing session in the interactive chat, exactly as
/// `agent chat --session-id <id>` would. Used by the sessions browser's resume
/// action so it reuses the one chat-launch path instead of reimplementing it.
pub(crate) fn resume_chat(
    config: CliConfig,
    defaults: &AgentSettings,
    session_id: &str,
) -> CliResult<String> {
    agent_chat(
        config,
        defaults,
        &["--session-id".to_string(), session_id.to_string()],
    )
}

fn agent_chat(config: CliConfig, defaults: &AgentSettings, args: &[String]) -> CliResult<String> {
    let options = parse_agent_chat_options(defaults, args)?;
    // The full-screen TUI is the primary interface on an interactive terminal.
    // Machine-readable modes (`--json-events`/`--stream-events`) and non-TTY
    // stdout (pipes, CI) fall back to the plain line REPL so scripted usage and
    // output redirection keep working unchanged.
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdout())
        && std::io::IsTerminal::is_terminal(&std::io::stdin())
        && !options.json_events
        && !options.stream_events;
    if interactive {
        crate::tui::run_agent_tui(config, options)
    } else {
        run_agent_chat(config, options)
    }
}
