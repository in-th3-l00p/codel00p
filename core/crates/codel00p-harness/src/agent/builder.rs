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
    memory_candidate_sink: Option<Arc<dyn MemoryCandidateSink>>,
    skill_extractor: Option<Arc<dyn SkillExtractor>>,
    skill_proposal_sink: Option<Arc<dyn SkillProposalSink>>,
    context_window: Option<ContextWindowState>,
    token_sink: Option<Arc<dyn TokenSink>>,
    max_iterations: Option<u32>,
    cancel: Option<CancelSignal>,
}

impl AgentHarnessBuilder {
    pub fn model_client<T>(mut self, model_client: T) -> Self
    where
        T: ModelClient + 'static,
    {
        self.model_client = Some(Arc::new(model_client));
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

    /// Wires a cancellation signal the run loop polls at turn boundaries, so a
    /// caller (e.g. a Ctrl-C handler) can stop a turn and keep partial progress.
    pub fn cancel_signal(mut self, cancel: CancelSignal) -> Self {
        self.cancel = Some(cancel);
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
        Ok(AgentHarness {
            model_client: self
                .model_client
                .ok_or_else(|| HarnessError::Configuration {
                    message: "model client is required".to_string(),
                })?,
            workspace: self.workspace.ok_or_else(|| HarnessError::Configuration {
                message: "workspace is required".to_string(),
            })?,
            tools: self.tools.unwrap_or_default(),
            permission_policy: self
                .permission_policy
                .unwrap_or_else(|| Arc::new(AllowAllPermissionPolicy)),
            event_sink: self.event_sink,
            lifecycle_hooks: self.lifecycle_hooks,
            project_memory_provider: self.project_memory_provider,
            skill_provider: self.skill_provider,
            turn_memory_extractor: self.turn_memory_extractor,
            memory_candidate_sink: self.memory_candidate_sink,
            skill_extractor: self.skill_extractor,
            skill_proposal_sink: self.skill_proposal_sink,
            context_window: self.context_window,
            token_sink: self.token_sink,
            max_iterations: self.max_iterations.unwrap_or(4),
            cancel: self.cancel.unwrap_or_default(),
        })
    }
}
