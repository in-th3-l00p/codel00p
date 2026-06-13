use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use codel00p_cron::CronJob;
use codel00p_gateway::{
    GatewayCommand,
    approvals::{ApprovalOutcome, ApprovalStore},
};
use codel00p_harness::{
    AgentEventSink, AgentHarness, AgentRole, DelegatedTask, DelegationOutcome,
    ExplicitTurnMemoryExtractor, HarnessError, HarnessEvent, MemoryCandidateSink,
    MemoryCandidateSinkOutcome, PermissionDecision, PermissionMode, PermissionPolicy,
    PermissionRequest, PermissionScope, ProcedureSkillExtractor, ProjectMemoryContext,
    ProjectMemoryItem, ProjectMemoryProvider, ProjectMemoryRequest, ProposedSkill,
    ProviderModelClient, SessionId, SkillContext, SkillPrompt, SkillProposalSink, SkillProvider,
    SkillSelectionRequest, SubAgentSpawner, TokenSink, ToolRegistry, UserMessage, Workspace,
    delegation_tools, learning_tools,
};
use codel00p_mcp::{
    HttpServerEndpoint, McpClient, McpHttpClient, McpStdioClient, McpTool, McpToolDescriptor,
    StdioServerCommand,
};
use codel00p_memory::{MemoryCandidateInput, MemoryError, MemoryQuery, MemoryRepository};
use codel00p_plugin::PluginRegistry;
use codel00p_protocol::AgentEvent;
use codel00p_providers::{ProviderRegistry, default_registry};
use codel00p_session::{SessionMetadata, SessionRecord, SessionStore, SessionStoreError};
use codel00p_skill::{
    SkillError, SkillProposal, SkillSource, load_skills, propose_skill, record_skill_usage,
    select_skills,
};

use crate::{
    config::{
        CliConfig, CliResult, open_memory_store, open_session_store, parse_session_id,
        required_value,
    },
    connector_permissions::{
        ConnectorPermissionDecision, ConnectorPermissionStatus, is_rememberable_permission,
        load_decision, remember_decision,
    },
    providers::build_provider_client_with,
    session::{session_message_summary, session_role_label},
    settings::AgentSettings,
};

struct AgentRunOptions {
    prompt: String,
    workspace: PathBuf,
    provider: String,
    model: String,
    provider_policy_preset: Option<String>,
    base_url: Option<String>,
    session_id: Option<String>,
    max_iterations: Option<u32>,
    json_events: bool,
    stream_events: bool,
    stream: bool,
    tool_sets: Vec<AgentToolSet>,
    permission_mode: CliPermissionMode,
    remember_permissions: bool,
    mcp_servers: Vec<McpServerSpec>,
    /// When set, the turn is a messaging-gateway turn: privileged tools pause
    /// for a remote chat user's `/approve` decision instead of using the local
    /// CLI permission mode. See [`GatewayApprovalPolicy`].
    gateway_approval: Option<GatewayApproval>,
}

/// Routes a gateway turn's privileged-tool permissions through a remote chat
/// user's `/approve` / `/deny` decisions, backed by a file [`ApprovalStore`].
#[derive(Clone)]
struct GatewayApproval {
    store: ApprovalStore,
    conversation: String,
    /// A one-shot grant: the single tool the remote user just approved may run
    /// once without re-prompting. Any *other* privileged tool re-prompts.
    granted_tool: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentToolSet {
    Read,
    Edit,
    Command,
    Git,
    Delegate,
    Learn,
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

pub fn run(config: CliConfig, defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    // No subcommand opens the interactive chat — the primary UI.
    let Some((command, rest)) = args.split_first() else {
        return agent_chat(config, &defaults, &[]);
    };

    match command.as_str() {
        "run" => agent_run(config, &defaults, rest),
        "resume" => agent_resume(config, &defaults, rest),
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
        permission_mode: CliPermissionMode::Allow,
        remember_permissions: false,
        mcp_servers: Vec::new(),
        gateway_approval: None,
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
        permission_mode: CliPermissionMode::Allow,
        remember_permissions: false,
        mcp_servers: Vec::new(),
        gateway_approval: Some(GatewayApproval {
            store: store.clone(),
            conversation: conversation.to_string(),
            granted_tool,
        }),
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

fn agent_chat(config: CliConfig, defaults: &AgentSettings, args: &[String]) -> CliResult<String> {
    let options = parse_agent_chat_options(defaults, args)?;
    run_agent_chat(config, options)
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
        let mut mcp_servers = load_mcp_servers_from_workspace(&options.workspace)?;
        mcp_servers.extend(options.mcp_servers.clone());

        let (session_state, previous_message_count) =
            prepare_session_state(&config, &options, session_mode)?;

        let harness =
            build_agent_harness(&config, &options, &mcp_servers, session_state.session_id())
                .await?;
        let outcome = harness
            .run_turn_with_state(session_state, UserMessage::new(options.prompt.clone()))
            .await
            .map_err(|error| {
                crate::error_help::humanize_provider_error(
                    &error.to_string(),
                    &options.provider,
                    &options.model,
                )
            })?;

        let mut output = String::new();
        if let Some(message) = &outcome.assistant_message {
            if options.stream {
                // The assistant text was already streamed to stdout token by
                // token; just terminate the streamed line.
                output.push('\n');
            } else {
                output.push_str(message);
                output.push('\n');
            }
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

/// Builds a fresh `AgentHarness` from the parsed run options. The harness is
/// consumed by `run_turn_with_state`, so interactive chat rebuilds one per turn.
async fn build_agent_harness(
    config: &CliConfig,
    options: &AgentRunOptions,
    mcp_servers: &[McpServerSpec],
    parent_session_id: &SessionId,
) -> CliResult<AgentHarness> {
    // Plugins are loaded once and contribute to providers, tools, and hooks.
    let plugins = load_plugins(&options.workspace)?;

    let provider_registry = plugins.apply_to_provider_registry(default_registry());
    let provider_client = build_provider_client_with(
        provider_registry.clone(),
        &options.provider,
        options.provider_policy_preset.as_deref(),
    )?;
    let model_client = ProviderModelClient::new(provider_client, &options.provider, &options.model);
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

    let mut tools =
        plugins.apply_to_tool_registry(build_tool_registry(&options.tool_sets, mcp_servers).await?);
    if options.tool_sets.contains(&AgentToolSet::Delegate) {
        let max_children = crate::settings::load_layered(&options.workspace)
            .ok()
            .and_then(|resolved| resolved.merged.delegation.max_concurrent_children)
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MAX_CONCURRENT_CHILDREN);
        let spawner = Arc::new(CliSubAgentSpawner {
            config: config.clone(),
            parent_session_id: parent_session_id.clone(),
            workspace: options.workspace.clone(),
            provider_registry,
            provider: options.provider.clone(),
            model: options.model.clone(),
            base_url: options.base_url.clone(),
            policy_preset: options.provider_policy_preset.clone(),
            max_iterations: options
                .max_iterations
                .unwrap_or(CHILD_DEFAULT_MAX_ITERATIONS),
            concurrency: Arc::new(tokio::sync::Semaphore::new(max_children as usize)),
        });
        tools = tools.with_registry(delegation_tools(AgentRole::Orchestrator, spawner));
    }
    if options.tool_sets.contains(&AgentToolSet::Learn) {
        let sink = Arc::new(CliSkillProposalSink::new(crate::skills::user_skills_dir()));
        tools = tools.with_registry(learning_tools(sink));
    }

    let mut builder = AgentHarness::builder()
        .model_client(model_client)
        .workspace(workspace)
        .tools(tools);
    // A gateway turn routes privileged-tool permissions to a remote chat user's
    // `/approve` decision; everything else uses the local CLI permission mode.
    builder = match &options.gateway_approval {
        Some(approval) => builder.permission_policy(GatewayApprovalPolicy::new(
            approval.store.clone(),
            approval.conversation.clone(),
            approval.granted_tool.clone(),
        )),
        None => builder.permission_policy(CliPermissionPolicy::new(
            config.clone(),
            options.permission_mode,
            options.remember_permissions,
        )),
    };
    let mut builder = builder
        .project_memory_provider(memory_provider)
        .skill_provider(CliSkillProvider::new(crate::skills::skill_sources(
            &options.workspace,
        )))
        .turn_memory_extractor(memory_extractor)
        .memory_candidate_sink(memory_sink);
    if options.tool_sets.contains(&AgentToolSet::Learn) {
        // The agent can both explicitly propose skills (the tool above) and have
        // procedures auto-extracted from completed multi-step turns. Both land in
        // the same review queue.
        builder = builder
            .skill_extractor(ProcedureSkillExtractor::default())
            .skill_proposal_sink(CliSkillProposalSink::new(crate::skills::user_skills_dir()));
    }
    builder = plugins.apply_to_harness_builder(builder);
    if options.stream_events {
        builder = builder.event_sink(StdoutJsonEventSink);
    }
    if options.stream {
        builder = builder.token_sink(StdoutTokenSink);
    }
    if let Some(max_iterations) = options.max_iterations {
        builder = builder.max_iterations(max_iterations);
    }

    builder.build().map_err(|error| error.to_string())
}

fn run_agent_chat(config: CliConfig, mut options: AgentRunOptions) -> CliResult<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut mcp_servers = load_mcp_servers_from_workspace(&options.workspace)?;
        mcp_servers.extend(options.mcp_servers.clone());

        // A bare `codel00p` chat starts a fresh conversation each launch. Resuming
        // is explicit (`--session-id`, or `/sessions` to find one) — never the
        // implicit default, which `SessionId::default()` would collapse onto a
        // single process-counter id (`session-1`) shared across every launch,
        // replaying an unbounded history until it overflows the context window.
        let session_id = match options.session_id.as_deref() {
            Some(value) => parse_session_id(value)?,
            None => parse_session_id(&fresh_chat_session_id())?,
        };
        let (mut session_state, mut persisted_message_count) =
            load_chat_session_state(&config, session_id)?;

        let mut stderr = io::stderr();
        writeln!(
            stderr,
            "codel00p chat — provider {} model {} (session {})",
            options.provider,
            options.model,
            session_state.session_id().as_str()
        )
        .ok();
        if persisted_message_count > 0 {
            writeln!(
                stderr,
                "Resumed conversation with {persisted_message_count} prior message(s)."
            )
            .ok();
        }
        writeln!(
            stderr,
            "Type a message and press Enter. Use /help for commands, /exit to quit."
        )
        .ok();

        loop {
            write!(stderr, "\nyou> ").ok();
            stderr.flush().ok();

            let mut line = String::new();
            let bytes = io::stdin()
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            if bytes == 0 {
                writeln!(stderr, "\nGoodbye.").ok();
                break;
            }

            let prompt = line.trim();
            if prompt.is_empty() {
                continue;
            }

            if let Some(command) = prompt.strip_prefix('/') {
                let (name, argument) = split_chat_command(command);
                match name {
                    "sessions" => {
                        write!(stderr, "{}", chat_sessions_listing(&config)?).ok();
                        continue;
                    }
                    "history" => {
                        write!(stderr, "{}", chat_history_listing(&session_state)).ok();
                        continue;
                    }
                    "tools" => {
                        let registry =
                            build_tool_registry(&options.tool_sets, &mcp_servers).await?;
                        write!(stderr, "{}", chat_tools_listing(&registry)).ok();
                        continue;
                    }
                    "model" => {
                        match argument {
                            Some(model) => {
                                options.model = model.to_string();
                                writeln!(stderr, "Model set to {}.", options.model).ok();
                            }
                            None => {
                                writeln!(
                                    stderr,
                                    "model {} (provider {})",
                                    options.model, options.provider
                                )
                                .ok();
                            }
                        }
                        continue;
                    }
                    "memory" => {
                        write!(stderr, "{}", chat_memory_listing(&config)?).ok();
                        continue;
                    }
                    _ => match handle_chat_command(
                        name,
                        &mut session_state,
                        &mut persisted_message_count,
                        &mut stderr,
                    ) {
                        ChatControl::Continue => continue,
                        ChatControl::Exit => break,
                    },
                }
            }

            let harness =
                build_agent_harness(&config, &options, &mcp_servers, session_state.session_id())
                    .await?;
            let outcome = match harness
                .run_turn_with_state(session_state.clone(), UserMessage::new(prompt.to_string()))
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    // A failed turn (e.g. an unsupported model) explains itself and
                    // keeps the chat open — `session_state` is unchanged (we passed
                    // a clone), so the user can `/model <id>` and retry instead of
                    // losing the whole conversation.
                    writeln!(
                        stderr,
                        "\n{}",
                        crate::error_help::humanize_provider_error(
                            &error.to_string(),
                            &options.provider,
                            &options.model,
                        )
                    )
                    .ok();
                    continue;
                }
            };

            if let Some(message) = &outcome.assistant_message {
                if options.stream {
                    // Tokens already streamed to stdout; end the line.
                    println!();
                } else {
                    println!("{message}");
                }
            } else {
                writeln!(stderr, "(no assistant response)").ok();
            }
            if options.json_events {
                for event in &outcome.events {
                    println!(
                        "{}",
                        serde_json::to_string(&event).map_err(|error| error.to_string())?
                    );
                }
            }

            persist_turn_outcome(
                &config,
                &outcome.session_state,
                &outcome.events,
                persisted_message_count,
            )?;
            persisted_message_count = outcome.session_state.messages().len();
            session_state = outcome.session_state;
        }

        Ok(String::new())
    })
}

enum ChatControl {
    Continue,
    Exit,
}

fn handle_chat_command(
    command: &str,
    session_state: &mut codel00p_harness::SessionState,
    persisted_message_count: &mut usize,
    stderr: &mut io::Stderr,
) -> ChatControl {
    let name = command.trim();
    match name {
        "exit" | "quit" | "q" => {
            writeln!(stderr, "Goodbye.").ok();
            ChatControl::Exit
        }
        "help" | "?" => {
            writeln!(
                stderr,
                "Commands:\n  \
                 /help              Show this help\n  \
                 /session           Show the current session id\n  \
                 /sessions          List all persisted conversations\n  \
                 /history           Show the current conversation\n  \
                 /tools             List the tools available this turn\n  \
                 /model [id]        Show or switch the model for later turns\n  \
                 /memory            Show approved project memory in context\n  \
                 /reset             Start a new conversation\n  \
                 /exit, /quit       Leave the chat"
            )
            .ok();
            ChatControl::Continue
        }
        "session" => {
            writeln!(stderr, "session {}", session_state.session_id().as_str()).ok();
            ChatControl::Continue
        }
        "reset" | "clear" => {
            *session_state =
                codel00p_harness::SessionState::new(codel00p_harness::SessionId::default());
            *persisted_message_count = 0;
            writeln!(
                stderr,
                "Started a new conversation (session {}).",
                session_state.session_id().as_str()
            )
            .ok();
            ChatControl::Continue
        }
        other => {
            writeln!(stderr, "Unknown command: /{other}. Try /help.").ok();
            ChatControl::Continue
        }
    }
}

fn split_chat_command(command: &str) -> (&str, Option<&str>) {
    let command = command.trim();
    match command.split_once(char::is_whitespace) {
        Some((name, rest)) => {
            let rest = rest.trim();
            (name, if rest.is_empty() { None } else { Some(rest) })
        }
        None => (command, None),
    }
}

/// A unique session id for a freshly launched chat, so each launch is its own
/// conversation rather than colliding on the process-counter default.
fn fresh_chat_session_id() -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("chat-{stamp}")
}

fn load_chat_session_state(
    config: &CliConfig,
    session_id: codel00p_harness::SessionId,
) -> CliResult<(codel00p_harness::SessionState, usize)> {
    let store = open_session_store(config)?;
    match store.metadata(&session_id) {
        Ok(_) => {
            let session_state = replay_session_messages(config, session_id)?;
            let count = session_state.messages().len();
            Ok((session_state, count))
        }
        Err(SessionStoreError::SessionNotFound { .. }) => {
            Ok((codel00p_harness::SessionState::new(session_id), 0))
        }
        Err(error) => Err(error.to_string()),
    }
}

fn chat_sessions_listing(config: &CliConfig) -> CliResult<String> {
    let store = open_session_store(config)?;
    let sessions = store.list_sessions().map_err(|error| error.to_string())?;
    if sessions.is_empty() {
        return Ok("No saved conversations yet.\n".to_string());
    }

    let mut output = String::new();
    for metadata in sessions {
        let messages = store
            .replay(metadata.session_id())
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|record| matches!(record.record(), SessionRecord::Message(_)))
            .count();
        output.push_str(&format!(
            "  {}\t{}\t{} message(s)\n",
            metadata.session_id().as_str(),
            metadata.source(),
            messages
        ));
    }
    Ok(output)
}

fn chat_history_listing(session_state: &codel00p_harness::SessionState) -> String {
    let messages = session_state.messages();
    if messages.is_empty() {
        return "No messages in this conversation yet.\n".to_string();
    }

    let mut output = String::new();
    for message in messages {
        let role = session_role_label(message.role());
        let summary = session_message_summary(message);
        output.push_str(&format!("  {role}: {summary}\n"));
    }
    output
}

fn chat_tools_listing(registry: &ToolRegistry) -> String {
    let names = registry.names();
    if names.is_empty() {
        return "No tools enabled. Use --tool-set to enable some.\n".to_string();
    }

    let mut names = names;
    names.sort();
    let mut output = String::new();
    for name in names {
        output.push_str(&format!("  {name}\n"));
    }
    output
}

fn chat_memory_listing(config: &CliConfig) -> CliResult<String> {
    let store = open_memory_store(config)?;
    let items = store
        .retrieve(MemoryQuery::new(config.project.clone()).with_limit(10))
        .map_err(|error| error.to_string())?;
    if items.is_empty() {
        return Ok("No approved project memory yet.\n".to_string());
    }

    let mut output = String::new();
    for memory in items {
        let entry = memory.entry();
        output.push_str(&format!(
            "  {}\t{:?}\t{}\n",
            entry.id(),
            entry.kind(),
            entry.content()
        ));
    }
    Ok(output)
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

fn parse_agent_run_options(
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

fn parse_agent_chat_options(
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

fn parse_agent_resume_options(
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
        "delegate" | "delegation" => Ok(AgentToolSet::Delegate),
        "learn" | "learning" => Ok(AgentToolSet::Learn),
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

/// Default iteration budget for a child agent when the parent run set none.
const CHILD_DEFAULT_MAX_ITERATIONS: u32 = 4;

/// Default cap on children running at once when config sets none.
const DEFAULT_MAX_CONCURRENT_CHILDREN: u32 = 4;

/// Runs a child agent for a delegated task, reusing the orchestrator's provider
/// configuration. Children run with read-only tools and as leaves (no further
/// delegation), which bounds blast radius and depth for this first wiring.
///
/// Each child session is persisted with the orchestrator's session as its
/// `parent`, so the agent tree is queryable and auditable. A shared semaphore
/// caps how many children run concurrently when the model delegates in a batch.
struct CliSubAgentSpawner {
    config: CliConfig,
    parent_session_id: SessionId,
    workspace: PathBuf,
    provider_registry: ProviderRegistry,
    provider: String,
    model: String,
    base_url: Option<String>,
    policy_preset: Option<String>,
    max_iterations: u32,
    concurrency: Arc<tokio::sync::Semaphore>,
}

#[async_trait]
impl SubAgentSpawner for CliSubAgentSpawner {
    async fn spawn(&self, task: DelegatedTask) -> Result<DelegationOutcome, HarnessError> {
        // Cap concurrent children even when the harness fires a delegate batch.
        let _permit =
            self.concurrency
                .acquire()
                .await
                .map_err(|error| HarnessError::Configuration {
                    message: format!("delegation concurrency limiter closed: {error}"),
                })?;

        let provider_client = build_provider_client_with(
            self.provider_registry.clone(),
            &self.provider,
            self.policy_preset.as_deref(),
        )
        .map_err(|message| HarnessError::Configuration { message })?;
        let model_client = ProviderModelClient::new(provider_client, &self.provider, &self.model);
        let model_client = match &self.base_url {
            Some(base_url) => model_client.with_base_url(base_url),
            None => model_client,
        };

        let workspace = Workspace::new(&self.workspace)?;
        let child_session_id = SessionId::new();

        let outcome = AgentHarness::builder()
            .model_client(model_client)
            .workspace(workspace)
            .tools(ToolRegistry::read_only_defaults())
            .max_iterations(self.max_iterations)
            .build()?
            .run_turn(
                child_session_id.clone(),
                UserMessage::new(task.description()),
            )
            .await?;

        // Record the child as its own session linked to the orchestrator, so the
        // delegation is visible via `session show` and the audit trail.
        persist_session_records(
            &self.config,
            &outcome.session_state,
            &outcome.events,
            0,
            "subagent",
            Some(self.parent_session_id.clone()),
        )
        .map_err(|message| HarnessError::Configuration { message })?;

        Ok(DelegationOutcome::new(
            outcome.assistant_message.unwrap_or_default(),
            child_session_id,
            outcome.tool_calls.len(),
        ))
    }
}

/// Assemble the plugins active for an agent run from layered configuration.
///
/// Reads `[plugins] enabled` from the workspace's resolved settings and builds a
/// registry from the built-in catalog. Enabled ids the catalog does not know are
/// skipped with a warning rather than failing the run, so a stale config entry
/// never bricks the agent. With no plugins enabled this returns an empty
/// registry, leaving the default tool/hook behaviour unchanged.
fn load_plugins(workspace: &Path) -> CliResult<PluginRegistry> {
    let resolved = crate::settings::load_layered(workspace)?;
    let catalog = crate::plugins::builtin_catalog();
    let enabled = resolved.merged.plugins.enabled.clone().unwrap_or_default();

    let (known, unknown): (Vec<String>, Vec<String>) =
        enabled.into_iter().partition(|id| catalog.contains(id));
    if !unknown.is_empty() {
        eprintln!(
            "warning: ignoring unknown plugin(s) in config: {}",
            unknown.join(", ")
        );
    }

    catalog.build(&known).map_err(|error| error.to_string())
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
            // Delegation needs the provider/model config to build a spawner, so
            // it is folded in by `build_agent_harness`, not here.
            AgentToolSet::Delegate => registry,
            // Learning needs the skills directory to record proposals, so it is
            // folded in by `build_agent_harness`, not here.
            AgentToolSet::Learn => registry,
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

/// A permission policy for messaging-gateway turns: a remote chat user grants
/// privileged tools out-of-band via `/approve`.
///
/// Read-only tools run freely. For any other scope the policy records a pending
/// request in the [`ApprovalStore`] and *denies* the call, pausing the turn —
/// the user is then prompted to `/approve` or `/deny`. An approval re-runs the
/// turn carrying a single one-shot `granted_tool` grant, so exactly the approved
/// tool may run once; anything further prompts again.
struct GatewayApprovalPolicy {
    store: ApprovalStore,
    conversation: String,
    /// `Some(tool)` until the matching tool runs once, then taken.
    granted_tool: Mutex<Option<String>>,
}

impl GatewayApprovalPolicy {
    fn new(store: ApprovalStore, conversation: String, granted_tool: Option<String>) -> Self {
        Self {
            store,
            conversation,
            granted_tool: Mutex::new(granted_tool),
        }
    }
}

/// A short, single-line description of what a tool wants to do, shown to the
/// remote user in the approval prompt.
fn describe_permission_request(request: &PermissionRequest) -> String {
    let input = request.input();
    // Prefer a `command` field (shell) for a crisp summary; otherwise show the
    // compact tool input, truncated so a chat prompt stays readable.
    if let Some(command) = input.get("command").and_then(|value| value.as_str()) {
        return command.trim().to_string();
    }
    let mut rendered = match input {
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    const MAX: usize = 200;
    if rendered.chars().count() > MAX {
        rendered = rendered.chars().take(MAX).collect::<String>() + "…";
    }
    rendered
}

#[async_trait]
impl PermissionPolicy for GatewayApprovalPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        // Reading never needs a remote user's blessing.
        if request.scope() == PermissionScope::ReadOnly {
            return Ok(PermissionDecision::allow(
                request.id(),
                PermissionMode::Allow,
            ));
        }
        // Consume a one-shot grant for exactly the tool the user just approved.
        {
            let mut granted = self
                .granted_tool
                .lock()
                .map_err(|_| HarnessError::ToolFailed {
                    name: request.tool_name().to_string(),
                    message: "gateway approval lock was poisoned".to_string(),
                })?;
            if granted.as_deref() == Some(request.tool_name()) {
                *granted = None;
                return Ok(PermissionDecision::allow(request.id(), PermissionMode::Ask));
            }
        }
        // Otherwise park the turn: record what is wanted and deny for now.
        self.store
            .record(
                &self.conversation,
                request.tool_name(),
                &describe_permission_request(&request),
            )
            .map_err(|error| HarnessError::ToolFailed {
                name: request.tool_name().to_string(),
                message: format!("failed to record approval request: {error}"),
            })?;
        Ok(PermissionDecision::deny(
            request.id(),
            PermissionMode::Ask,
            format!("awaiting remote /approve for {}", request.tool_name()),
        ))
    }
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

struct StdoutTokenSink;

impl TokenSink for StdoutTokenSink {
    fn on_token(&self, token: &str) {
        print!("{token}");
        let _ = io::stdout().flush();
    }
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
    persist_session_records(
        config,
        session_state,
        events,
        message_start_index,
        "cli",
        None,
    )
}

/// Persist a session's new messages and events, creating it with `source` and an
/// optional `parent` for lineage (used for sub-agent child sessions).
fn persist_session_records(
    config: &CliConfig,
    session_state: &codel00p_harness::SessionState,
    events: &[AgentEvent],
    message_start_index: usize,
    source: &str,
    parent: Option<SessionId>,
) -> CliResult<()> {
    let mut store = open_session_store(config)?;
    let mut metadata = SessionMetadata::new(session_state.session_id().clone(), source);
    if let Some(parent) = parent {
        metadata = metadata.with_parent(parent);
    }
    match store.create_session(metadata) {
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

/// Default number of skills injected into a turn.
const SKILL_SELECTION_LIMIT: usize = 3;

/// Selects locally-authored skills relevant to the turn and hands them to the
/// harness for injection. Loading is filesystem-only, so it runs inline.
///
/// Each selected skill's usage is recorded once per turn. The provider is built
/// fresh per turn, so `recorded` deduplicates across the agentic loop's
/// iterations (which each call `select`).
struct CliSkillProvider {
    sources: Vec<(SkillSource, PathBuf)>,
    limit: usize,
    recorded: Mutex<HashSet<String>>,
}

impl CliSkillProvider {
    fn new(sources: Vec<(SkillSource, PathBuf)>) -> Self {
        Self {
            sources,
            limit: SKILL_SELECTION_LIMIT,
            recorded: Mutex::new(HashSet::new()),
        }
    }

    /// True the first time `name` is seen this turn, so usage is counted once.
    fn first_use_this_turn(&self, name: &str) -> bool {
        self.recorded
            .lock()
            .expect("usage lock")
            .insert(name.to_string())
    }
}

#[async_trait]
impl SkillProvider for CliSkillProvider {
    async fn select(&self, request: SkillSelectionRequest) -> Result<SkillContext, HarnessError> {
        let skills = load_skills(&self.sources);
        let selected = select_skills(&skills, request.query(), self.limit);
        let now = now_epoch_secs();

        let prompts = selected
            .into_iter()
            .map(|skill| {
                if self.first_use_this_turn(&skill.name) {
                    // Best-effort: usage tracking must never fail a turn.
                    let _ = record_skill_usage(&skill, now);
                }
                SkillPrompt::new(skill.name, skill.body)
            })
            .collect();
        Ok(SkillContext::new(prompts))
    }
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Records agent-proposed skills as review candidates under the user skills dir.
/// Proposals stay inactive until a human runs `codel00p skills approve`.
struct CliSkillProposalSink {
    skills_dir: PathBuf,
}

impl CliSkillProposalSink {
    fn new(skills_dir: PathBuf) -> Self {
        Self { skills_dir }
    }
}

#[async_trait]
impl SkillProposalSink for CliSkillProposalSink {
    async fn propose(&self, skill: ProposedSkill) -> Result<(), HarnessError> {
        let proposal = SkillProposal {
            name: skill.name().to_string(),
            description: skill.description().to_string(),
            triggers: skill.triggers().to_vec(),
            instructions: skill.instructions().to_string(),
            created_by: "agent".to_string(),
        };
        match propose_skill(&self.skills_dir, &proposal) {
            // Idempotent: a name already proposed or active is a benign no-op,
            // so repeated tasks (e.g. automatic extraction) stay quiet.
            Ok(_)
            | Err(SkillError::CandidateExists { .. })
            | Err(SkillError::AlreadyActive { .. }) => Ok(()),
            Err(error) => Err(HarnessError::Configuration {
                message: error.to_string(),
            }),
        }
    }
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
                Err(
                    MemoryError::MemoryAlreadyExists { .. } | MemoryError::DuplicateMemory { .. },
                ) => duplicate_ids.push(id),
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

#[cfg(test)]
mod tests {
    use super::*;

    use codel00p_protocol::ProjectRef;
    use codel00p_session::SessionStore;
    use httpmock::{Method::POST, MockServer};
    use serde_json::json;

    fn test_config(dir: &std::path::Path) -> CliConfig {
        CliConfig {
            memory_db: dir.join("memory.sqlite"),
            organization_id: "test-org".to_string(),
            project: ProjectRef::new("test-project", "Test Project"),
        }
    }

    // A child agent run goes through the real provider transport, so mock one
    // chat-completions response and confirm the spawner runs a child, returns
    // its summary, and records the child session linked to its parent.
    #[test]
    fn cli_spawner_runs_a_child_and_records_lineage() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(json!({
                "choices": [{
                    "message": { "role": "assistant", "content": "child summary" },
                    "finish_reason": "stop"
                }]
            }));
        });

        let dir = tempfile::tempdir().expect("tempdir");
        let config = test_config(dir.path());
        let parent_session_id = SessionId::from_static("parent-session");
        // SAFETY: guarded by LOCK so no other test mutates this var concurrently.
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token");
        }

        let spawner = CliSubAgentSpawner {
            config: config.clone(),
            parent_session_id: parent_session_id.clone(),
            workspace: dir.path().to_path_buf(),
            provider_registry: default_registry(),
            provider: "custom".to_string(),
            model: "test-model".to_string(),
            base_url: Some(server.base_url()),
            policy_preset: None,
            max_iterations: 2,
            concurrency: Arc::new(tokio::sync::Semaphore::new(2)),
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let outcome = runtime
            .block_on(spawner.spawn(DelegatedTask::new("summarize the project")))
            .expect("spawn child");

        // SAFETY: still under LOCK.
        unsafe {
            std::env::remove_var("CODEL00P_PROVIDER_CUSTOM_API_KEY");
        }

        mock.assert();
        assert_eq!(outcome.summary(), "child summary");
        assert_eq!(outcome.tool_calls(), 0);

        // The child session is persisted with the parent as its lineage.
        let store = open_session_store(&config).expect("session store");
        let metadata = store
            .metadata(outcome.child_session_id())
            .expect("child session persisted");
        assert_eq!(metadata.parent_session_id(), Some(&parent_session_id));
    }
}
