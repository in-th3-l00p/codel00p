use std::{env, path::PathBuf};

use async_trait::async_trait;
use codel00p_harness::{
    AgentEventSink, AgentHarness, ExplicitTurnMemoryExtractor, HarnessError, HarnessEvent,
    MemoryCandidateSink, MemoryCandidateSinkOutcome, PermissionDecision, PermissionMode,
    PermissionPolicy, PermissionRequest, ProjectMemoryContext, ProjectMemoryItem,
    ProjectMemoryProvider, ProjectMemoryRequest, ProviderModelClient, ToolRegistry, UserMessage,
    Workspace,
};
use codel00p_memory::{MemoryCandidateInput, MemoryError, MemoryQuery, MemoryRepository};
use codel00p_protocol::AgentEvent;
use codel00p_session::{SessionMetadata, SessionRecord, SessionStore, SessionStoreError};

use crate::{
    config::{
        CliConfig, CliResult, open_memory_store, open_session_store, parse_session_id,
        required_value,
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

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing agent command".to_string());
    };

    match command.as_str() {
        "run" => agent_run(config, rest),
        "resume" => agent_resume(config, rest),
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

enum AgentSessionMode {
    Fresh,
    Resume,
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

        let mut builder = AgentHarness::builder()
            .model_client(model_client)
            .workspace(workspace)
            .tools(build_tool_registry(&options.tool_sets))
            .permission_policy(CliPermissionPolicy::new(options.permission_mode))
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

fn build_tool_registry(tool_sets: &[AgentToolSet]) -> ToolRegistry {
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
    registry
}

struct CliPermissionPolicy {
    mode: CliPermissionMode,
}

impl CliPermissionPolicy {
    fn new(mode: CliPermissionMode) -> Self {
        Self { mode }
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
            CliPermissionMode::Ask => Ok(PermissionDecision::deny(
                request.id(),
                PermissionMode::Ask,
                format!(
                    "{} requires approval, but interactive permission prompts are not implemented",
                    request.tool_name()
                ),
            )),
            CliPermissionMode::Deny => Ok(PermissionDecision::deny(
                request.id(),
                PermissionMode::Deny,
                format!("{} denied by CLI permission mode", request.tool_name()),
            )),
        }
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
