use std::{env, path::PathBuf};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, ExplicitTurnMemoryExtractor, MemoryCandidateSink, MemoryCandidateSinkOutcome,
    ProjectMemoryContext, ProjectMemoryItem, ProjectMemoryProvider, ProjectMemoryRequest,
    ProviderModelClient, ToolRegistry, UserMessage, Workspace,
};
use codel00p_memory::{MemoryCandidateInput, MemoryError, MemoryQuery, MemoryRepository};
use codel00p_protocol::AgentEvent;
use codel00p_session::{SessionMetadata, SessionStore, SessionStoreError};

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
}

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
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
        persist_turn_outcome(&config, &outcome.session_state, &outcome.events)?;

        Ok(output)
    })
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

fn persist_turn_outcome(
    config: &CliConfig,
    session_state: &codel00p_harness::SessionState,
    events: &[AgentEvent],
) -> CliResult<()> {
    let mut store = open_session_store(config)?;
    match store.create_session(SessionMetadata::new(
        session_state.session_id().clone(),
        "cli",
    )) {
        Ok(()) | Err(SessionStoreError::SessionAlreadyExists { .. }) => {}
        Err(error) => return Err(error.to_string()),
    }

    for message in session_state.messages() {
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
