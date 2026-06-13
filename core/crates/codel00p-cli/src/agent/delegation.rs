//! CLI-backed sub-agent spawning for delegate tool calls.

use super::*;

pub(super) const CHILD_DEFAULT_MAX_ITERATIONS: u32 = 4;
pub(super) const DEFAULT_MAX_CONCURRENT_CHILDREN: u32 = 4;

pub(super) struct CliSubAgentSpawner {
    pub(super) config: CliConfig,
    pub(super) parent_session_id: SessionId,
    pub(super) workspace: PathBuf,
    pub(super) provider_registry: ProviderRegistry,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) base_url: Option<String>,
    pub(super) policy_preset: Option<String>,
    pub(super) max_iterations: u32,
    pub(super) concurrency: Arc<tokio::sync::Semaphore>,
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
