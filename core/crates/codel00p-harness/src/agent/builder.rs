//! Builder for assembling an AgentHarness with optional runtime services.

use super::*;

impl AgentHarness {
    pub fn builder() -> AgentHarnessBuilder {
        AgentHarnessBuilder::default()
    }
}

#[derive(Default)]
pub struct AgentHarnessBuilder {
    model_client: Option<Arc<dyn ModelClient>>,
    workspace: Option<Workspace>,
    tools: Option<ToolRegistry>,
    permission_policy: Option<Arc<dyn PermissionPolicy>>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
    lifecycle_hooks: Vec<Arc<dyn LifecycleHook>>,
    project_memory_provider: Option<Arc<dyn ProjectMemoryProvider>>,
    skill_provider: Option<Arc<dyn SkillProvider>>,
    turn_memory_extractor: Option<Arc<dyn TurnMemoryExtractor>>,
    memory_recommender: Option<Arc<dyn MemoryRecommender>>,
    memory_candidate_sink: Option<Arc<dyn MemoryCandidateSink>>,
    skill_extractor: Option<Arc<dyn SkillExtractor>>,
    skill_proposal_sink: Option<Arc<dyn SkillProposalSink>>,
    context_window: Option<ContextWindowState>,
    token_sink: Option<Arc<dyn TokenSink>>,
    max_iterations: Option<u32>,
    max_tool_result_bytes: Option<usize>,
    tool_choice: Option<ToolChoice>,
    response_format: Option<ResponseFormat>,
    cancel: Option<CancelSignal>,
    programmatic_tooling: bool,
    capabilities: Vec<crate::capability::Capability>,
    capability_proposals: Option<Arc<dyn crate::capability::CapabilityProposalSink>>,
    capability_extractor: Option<Arc<dyn crate::capability::CapabilityExtractor>>,
}

impl AgentHarnessBuilder {
    pub fn model_client<T>(mut self, model_client: T) -> Self
    where
        T: ModelClient + 'static,
    {
        self.model_client = Some(Arc::new(model_client));
        self
    }

    /// Set an already type-erased model client (e.g. shared with a parent agent
    /// when spawning sub-agents).
    pub fn model_client_arc(mut self, model_client: Arc<dyn ModelClient>) -> Self {
        self.model_client = Some(model_client);
        self
    }

    pub fn workspace(mut self, workspace: Workspace) -> Self {
        self.workspace = Some(workspace);
        self
    }

    pub fn tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn permission_policy<T>(mut self, permission_policy: T) -> Self
    where
        T: PermissionPolicy + 'static,
    {
        self.permission_policy = Some(Arc::new(permission_policy));
        self
    }

    /// Set an already type-erased permission policy (e.g. the parent's ceiling
    /// applied to a spawned sub-agent).
    pub fn permission_policy_arc(mut self, permission_policy: Arc<dyn PermissionPolicy>) -> Self {
        self.permission_policy = Some(permission_policy);
        self
    }

    pub fn event_sink<T>(mut self, event_sink: T) -> Self
    where
        T: AgentEventSink + 'static,
    {
        self.event_sink = Some(Arc::new(event_sink));
        self
    }

    pub fn context_window(mut self, context_window: ContextWindowState) -> Self {
        self.context_window = Some(context_window);
        self
    }

    pub fn lifecycle_hook<T>(mut self, lifecycle_hook: T) -> Self
    where
        T: LifecycleHook + 'static,
    {
        self.lifecycle_hooks.push(Arc::new(lifecycle_hook));
        self
    }

    /// Add an already type-erased lifecycle hook.
    ///
    /// This is the entry point used when hooks are contributed dynamically (for
    /// example by a plugin) rather than by a statically typed `lifecycle_hook`
    /// call. Hooks run in the order they are added.
    pub fn lifecycle_hook_arc(mut self, lifecycle_hook: Arc<dyn LifecycleHook>) -> Self {
        self.lifecycle_hooks.push(lifecycle_hook);
        self
    }

    pub fn project_memory_provider<T>(mut self, project_memory_provider: T) -> Self
    where
        T: ProjectMemoryProvider + 'static,
    {
        self.project_memory_provider = Some(Arc::new(project_memory_provider));
        self
    }

    pub fn skill_provider<T>(mut self, skill_provider: T) -> Self
    where
        T: SkillProvider + 'static,
    {
        self.skill_provider = Some(Arc::new(skill_provider));
        self
    }

    pub fn turn_memory_extractor<T>(mut self, turn_memory_extractor: T) -> Self
    where
        T: TurnMemoryExtractor + 'static,
    {
        self.turn_memory_extractor = Some(Arc::new(turn_memory_extractor));
        self
    }

    /// Sets a recommender that runs after each completed turn to *automatically*
    /// propose memory candidates from the work — even when the agent emitted no
    /// explicit `remember:` directive. Recommendations land in the same review
    /// queue as explicit candidates (via `memory_candidate_sink`) and are never
    /// auto-approved, so set a sink too. With no recommender configured, the
    /// post-turn recommendation step is a no-op (backward compatible).
    pub fn memory_recommender(mut self, recommender: Arc<dyn MemoryRecommender>) -> Self {
        self.memory_recommender = Some(recommender);
        self
    }

    pub fn memory_candidate_sink<T>(mut self, memory_candidate_sink: T) -> Self
    where
        T: MemoryCandidateSink + 'static,
    {
        self.memory_candidate_sink = Some(Arc::new(memory_candidate_sink));
        self
    }

    pub fn skill_extractor<T>(mut self, skill_extractor: T) -> Self
    where
        T: SkillExtractor + 'static,
    {
        self.skill_extractor = Some(Arc::new(skill_extractor));
        self
    }

    pub fn skill_proposal_sink<T>(mut self, skill_proposal_sink: T) -> Self
    where
        T: SkillProposalSink + 'static,
    {
        self.skill_proposal_sink = Some(Arc::new(skill_proposal_sink));
        self
    }

    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    /// Caps the byte size of each tool result recorded for the model. Over-budget
    /// results become a head+tail preview with the full output saved to a temp
    /// file. `0` disables truncation. Default: 16 KiB.
    pub fn max_tool_result_bytes(mut self, max_bytes: usize) -> Self {
        self.max_tool_result_bytes = Some(max_bytes);
        self
    }

    /// Controls whether/which tool the model must call each turn. Without this the
    /// provider default (auto) applies.
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    /// Requests a structured (JSON) response each turn where the provider
    /// supports it (JSON mode), distinct from tool calling.
    pub fn response_format(mut self, response_format: ResponseFormat) -> Self {
        self.response_format = Some(response_format);
        self
    }

    /// Wires a cancellation signal the run loop polls at turn boundaries, so a
    /// caller (e.g. a Ctrl-C handler) can stop a turn and keep partial progress.
    pub fn cancel_signal(mut self, cancel: CancelSignal) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Adds the `run_pipeline` tool (programmatic tool calling): the model can
    /// run a declared multi-step tool pipeline in one inference. Each step is
    /// dispatched through this harness's own tool registry and permission policy,
    /// so governance is identical to a direct tool call — only orchestration
    /// moves into the pipeline. The pipeline calls the tool set as configured at
    /// build time (minus `run_pipeline` itself, so pipelines do not nest).
    pub fn programmatic_tooling(mut self, enabled: bool) -> Self {
        self.programmatic_tooling = enabled;
        self
    }

    /// Register approved synthesized capabilities as callable tools. Each runs
    /// its frozen pipeline through this harness's tool set and permission policy,
    /// so a capability is governed exactly like the direct tool calls it wraps.
    pub fn capabilities(mut self, capabilities: Vec<crate::capability::Capability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Adds the `propose_capability` tool, letting the agent submit a pipeline to
    /// `sink` (a review queue) to be frozen into a future capability. The same
    /// sink also receives any auto-extracted proposals.
    pub fn capability_proposals(
        mut self,
        sink: Arc<dyn crate::capability::CapabilityProposalSink>,
    ) -> Self {
        self.capability_proposals = Some(sink);
        self
    }

    /// Sets an extractor that runs after each completed turn to auto-propose a
    /// capability from the work (closing the synthesis loop without the agent
    /// explicitly calling `propose_capability`). Proposals go to the
    /// `capability_proposals` sink, so set that too.
    pub fn capability_extractor(
        mut self,
        extractor: Arc<dyn crate::capability::CapabilityExtractor>,
    ) -> Self {
        self.capability_extractor = Some(extractor);
        self
    }

    /// Streams assistant text to `token_sink` as it is generated. Without a sink
    /// the harness uses the non-streaming inference path.
    pub fn token_sink<T>(mut self, token_sink: T) -> Self
    where
        T: TokenSink + 'static,
    {
        self.token_sink = Some(Arc::new(token_sink));
        self
    }

    pub fn build(self) -> Result<AgentHarness, HarnessError> {
        let model_client = self
            .model_client
            .ok_or_else(|| HarnessError::Configuration {
                message: "model client is required".to_string(),
            })?;
        let workspace = self.workspace.ok_or_else(|| HarnessError::Configuration {
            message: "workspace is required".to_string(),
        })?;
        let permission_policy = self
            .permission_policy
            .unwrap_or_else(|| Arc::new(AllowAllPermissionPolicy));

        // Programmatic tooling (run_pipeline) and synthesized capabilities both
        // dispatch their sub-steps through a snapshot of the tool set taken
        // *before* any of them are added — so they call the primitive tools, not
        // each other (no recursion) — and through this harness's own permission
        // policy, so every sub-step stays individually gated.
        let mut tools = self.tools.unwrap_or_default();
        let base = tools.clone();
        if self.programmatic_tooling {
            tools = tools.with_registry(crate::pipeline::pipeline_tools(
                base.clone(),
                permission_policy.clone(),
            ));
        }
        if !self.capabilities.is_empty() {
            let capabilities = crate::capability::capability_tools(
                base.clone(),
                permission_policy.clone(),
                self.capabilities,
            )?;
            tools = tools.with_registry(capabilities);
        }
        // The proposal sink backs both the `propose_capability` tool and the
        // post-turn auto-extractor, so explicit and automatic proposals land in
        // the same review queue.
        let capability_proposal_sink = self.capability_proposals;
        if let Some(sink) = &capability_proposal_sink {
            tools = tools.with_tool(crate::capability::ProposeCapabilityTool::new(
                Arc::new(base),
                sink.clone(),
            ));
        }

        Ok(AgentHarness {
            model_client,
            workspace,
            tools,
            permission_policy,
            event_sink: self.event_sink,
            lifecycle_hooks: self.lifecycle_hooks,
            project_memory_provider: self.project_memory_provider,
            skill_provider: self.skill_provider,
            turn_memory_extractor: self.turn_memory_extractor,
            memory_recommender: self.memory_recommender,
            memory_candidate_sink: self.memory_candidate_sink,
            capability_extractor: self.capability_extractor,
            capability_proposal_sink,
            skill_extractor: self.skill_extractor,
            skill_proposal_sink: self.skill_proposal_sink,
            context_window: self.context_window,
            token_sink: self.token_sink,
            max_iterations: self.max_iterations.unwrap_or(4),
            tool_output_truncation: match self.max_tool_result_bytes {
                Some(0) => ToolOutputTruncation::disabled(),
                Some(bytes) => ToolOutputTruncation::new(bytes),
                None => ToolOutputTruncation::new(16 * 1024),
            },
            tool_choice: self.tool_choice,
            response_format: self.response_format,
            cancel: self.cancel.unwrap_or_default(),
        })
    }
}
