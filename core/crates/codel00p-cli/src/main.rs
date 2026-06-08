use std::{env, path::PathBuf, process::ExitCode};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, ExplicitTurnMemoryExtractor, MemoryCandidateSink, MemoryCandidateSinkOutcome,
    ProjectMemoryContext, ProjectMemoryItem, ProjectMemoryProvider, ProjectMemoryRequest,
    ProviderModelClient, SessionId, ToolRegistry, UserMessage, Workspace,
};
use codel00p_memory::{
    MemoryCandidateInput, MemoryError, MemoryListFilter, MemoryQuery, MemoryRepository,
    ReviewDecision, StorageBackedMemoryStore,
};
use codel00p_protocol::{MemoryKind, MemoryStatus, ProjectRef};
use codel00p_providers::{Credential, InferenceClient, default_registry};
use codel00p_storage::{SqliteStorage, StorageScope};

type CliResult<T> = Result<T, String>;

#[derive(Clone)]
struct CliConfig {
    memory_db: PathBuf,
    organization_id: String,
    project: ProjectRef,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> CliResult<String> {
    let (config, rest) = parse_global_args(args)?;
    let Some((command, rest)) = rest.split_first() else {
        return Err("missing command".to_string());
    };

    match command.as_str() {
        "agent" => run_agent(config, rest),
        "memory" => run_memory(config, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}

fn parse_global_args(args: Vec<String>) -> CliResult<(CliConfig, Vec<String>)> {
    let mut memory_db = None;
    let mut organization_id = None;
    let mut project_id = None;
    let mut project_name = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--memory-db" => {
                memory_db = Some(PathBuf::from(required_value(&args, index, "--memory-db")?));
                index += 2;
            }
            "--organization-id" => {
                organization_id = Some(required_value(&args, index, "--organization-id")?);
                index += 2;
            }
            "--project-id" => {
                project_id = Some(required_value(&args, index, "--project-id")?);
                index += 2;
            }
            "--project-name" => {
                project_name = Some(required_value(&args, index, "--project-name")?);
                index += 2;
            }
            _ => break,
        }
    }

    let config = CliConfig {
        memory_db: memory_db.ok_or_else(|| "missing required --memory-db".to_string())?,
        organization_id: organization_id
            .ok_or_else(|| "missing required --organization-id".to_string())?,
        project: ProjectRef::new(
            project_id.ok_or_else(|| "missing required --project-id".to_string())?,
            project_name.ok_or_else(|| "missing required --project-name".to_string())?,
        ),
    };

    Ok((config, args[index..].to_vec()))
}

fn required_value(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

fn run_memory(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing memory command".to_string());
    };

    match command.as_str() {
        "list" => memory_list(config, rest),
        "show" => memory_show(config, rest),
        "audit" => memory_audit(config, rest),
        "approve" => memory_review(config, rest, ReviewCommand::Approve),
        "reject" => memory_review(config, rest, ReviewCommand::Reject),
        "archive" => memory_review(config, rest, ReviewCommand::Archive),
        _ => Err(format!("unknown memory command: {command}")),
    }
}

fn open_store(
    config: &CliConfig,
) -> CliResult<StorageBackedMemoryStore<codel00p_storage::SqliteStorage>> {
    let storage = SqliteStorage::open(&config.memory_db).map_err(|error| error.to_string())?;
    Ok(StorageBackedMemoryStore::new(
        StorageScope::project(&config.organization_id, config.project.id()),
        storage,
    ))
}

struct AgentRunOptions {
    prompt: String,
    workspace: PathBuf,
    provider: String,
    model: String,
    base_url: Option<String>,
    session_id: Option<String>,
    max_iterations: Option<u32>,
    json_events: bool,
}

fn run_agent(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing agent command".to_string());
    };

    match command.as_str() {
        "run" => agent_run(config, rest),
        _ => Err(format!("unknown agent command: {command}")),
    }
}

fn agent_run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let options = parse_agent_run_options(args)?;
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
            .tools(ToolRegistry::read_only_defaults())
            .project_memory_provider(memory_provider)
            .turn_memory_extractor(memory_extractor)
            .memory_candidate_sink(memory_sink);
        if let Some(max_iterations) = options.max_iterations {
            builder = builder.max_iterations(max_iterations);
        }

        let session_id = options
            .session_id
            .as_deref()
            .map(parse_session_id)
            .transpose()?
            .unwrap_or_default();
        let outcome = builder
            .build()
            .map_err(|error| error.to_string())?
            .run_turn(session_id, UserMessage::new(options.prompt))
            .await
            .map_err(|error| error.to_string())?;

        let mut output = String::new();
        if let Some(message) = outcome.assistant_message {
            output.push_str(&message);
            output.push('\n');
        }
        if options.json_events {
            for event in outcome.events {
                output.push_str(&serde_json::to_string(&event).map_err(|error| error.to_string())?);
                output.push('\n');
            }
        }

        Ok(output)
    })
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
        let store = open_store(&self.config)
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
        let mut store = open_store(&self.config)
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

fn parse_session_id(value: &str) -> CliResult<SessionId> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| format!("invalid --session-id: {error}"))
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
    })
}

fn build_provider_client(provider: &str) -> CliResult<InferenceClient> {
    let credential = provider_credential(provider)
        .ok_or_else(|| format!("missing credential for provider `{provider}`"))?;

    Ok(InferenceClient::builder()
        .registry(default_registry())
        .credential(provider, credential)
        .build())
}

fn provider_credential(provider: &str) -> Option<Credential> {
    provider_env_vars(provider)
        .iter()
        .find_map(|key| read_secret(key))
        .map(Credential::api_key)
}

fn provider_env_vars(provider: &str) -> Vec<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "github" | "github-copilot" | "copilot" => vec![
            "CODEL00P_PROVIDER_GITHUB_TOKEN",
            "COPILOT_GITHUB_TOKEN",
            "GH_TOKEN",
            "GITHUB_TOKEN",
        ],
        "openrouter" | "or" => vec!["CODEL00P_PROVIDER_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"],
        "openai" => vec!["CODEL00P_PROVIDER_OPENAI_API_KEY", "OPENAI_API_KEY"],
        "anthropic" | "claude" => vec![
            "CODEL00P_PROVIDER_ANTHROPIC_API_KEY",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_TOKEN",
        ],
        "azure" | "azure-foundry" => vec![
            "CODEL00P_PROVIDER_AZURE_FOUNDRY_API_KEY",
            "AZURE_FOUNDRY_API_KEY",
        ],
        "gemini" | "google" => vec![
            "CODEL00P_PROVIDER_GEMINI_API_KEY",
            "GOOGLE_API_KEY",
            "GEMINI_API_KEY",
        ],
        "custom" | "ollama" | "local" | "vllm" | "llamacpp" | "llama.cpp" | "llama-cpp" => {
            vec!["CODEL00P_PROVIDER_CUSTOM_API_KEY"]
        }
        _ => vec![],
    }
}

fn read_secret(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn memory_list(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut filter = MemoryListFilter::new(config.project.clone());
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--status" => {
                filter =
                    filter.with_status(parse_status(&required_value(args, index, "--status")?)?);
                index += 2;
            }
            "--kind" => {
                filter = filter.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--tag" => {
                filter = filter.with_tag(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                filter = filter.with_limit(limit);
                index += 2;
            }
            flag => return Err(format!("unknown memory list option: {flag}")),
        }
    }

    let store = open_store(&config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    let mut output = String::new();
    for record in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            record.entry().id(),
            status_label(record.entry().status()),
            kind_label(record.entry().kind()),
            record.entry().content()
        ));
    }
    Ok(output)
}

fn memory_show(config: CliConfig, args: &[String]) -> CliResult<String> {
    let id = single_id(args, "memory show")?;
    let store = open_store(&config)?;
    let record = store.get(id).map_err(|error| error.to_string())?;

    Ok(format!(
        "id: {}\nstatus: {}\nkind: {}\ntags: {}\ncontent: {}\n",
        record.entry().id(),
        status_label(record.entry().status()),
        kind_label(record.entry().kind()),
        record.entry().tags().join(","),
        record.entry().content()
    ))
}

fn memory_audit(config: CliConfig, args: &[String]) -> CliResult<String> {
    let id = single_id(args, "memory audit")?;
    let store = open_store(&config)?;
    let audit = store.audit_log(id).map_err(|error| error.to_string())?;
    let mut output = String::new();
    for event in audit {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            event.sequence(),
            audit_action_label(event.action()),
            event.actor(),
            event.reason().unwrap_or("")
        ));
    }
    Ok(output)
}

enum ReviewCommand {
    Approve,
    Reject,
    Archive,
}

fn memory_review(config: CliConfig, args: &[String], command: ReviewCommand) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut reason = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            flag => return Err(format!("unknown review option: {flag}")),
        }
    }

    let actor = actor.ok_or_else(|| "missing required --actor".to_string())?;
    let decision = match command {
        ReviewCommand::Approve => ReviewDecision::approve(actor),
        ReviewCommand::Reject => ReviewDecision::reject(
            actor,
            reason.ok_or_else(|| "missing required --reason".to_string())?,
        ),
        ReviewCommand::Archive => ReviewDecision::archive(
            actor,
            reason.ok_or_else(|| "missing required --reason".to_string())?,
        ),
    };

    let mut store = open_store(&config)?;
    let record = store
        .review(id, decision)
        .map_err(|error| error.to_string())?;

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

fn single_id<'a>(args: &'a [String], command: &str) -> CliResult<&'a str> {
    if args.len() != 1 {
        return Err(format!("{command} expects exactly one memory id"));
    }
    Ok(&args[0])
}

fn parse_status(value: &str) -> CliResult<MemoryStatus> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

fn parse_kind(value: &str) -> CliResult<MemoryKind> {
    match value {
        "architecture" => Ok(MemoryKind::Architecture),
        "convention" => Ok(MemoryKind::Convention),
        "workflow" => Ok(MemoryKind::Workflow),
        "decision" => Ok(MemoryKind::Decision),
        "deployment" => Ok(MemoryKind::Deployment),
        "troubleshooting" => Ok(MemoryKind::Troubleshooting),
        _ => Err(format!("unknown memory kind: {value}")),
    }
}

fn status_label(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Approved => "approved",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Archived => "archived",
    }
}

fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn audit_action_label(action: codel00p_memory::MemoryAuditAction) -> &'static str {
    match action {
        codel00p_memory::MemoryAuditAction::CandidateCreated => "candidate_created",
        codel00p_memory::MemoryAuditAction::Approved => "approved",
        codel00p_memory::MemoryAuditAction::Rejected => "rejected",
        codel00p_memory::MemoryAuditAction::Archived => "archived",
    }
}
