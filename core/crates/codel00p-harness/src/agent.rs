use std::sync::Arc;

use futures::future::join_all;

use crate::{
    cancel::CancelSignal,
    errors::HarnessError,
    event_sink::AgentEventSink,
    events::HarnessEvent,
    instructions::ProjectInstructionLoader,
    iteration_budget::IterationBudget,
    learning::{SkillExtractionRequest, SkillExtractor, SkillProposalSink},
    lifecycle::{LifecycleHook, TurnLifecycleContext},
    memory::{
        MemoryCandidateSink, MemoryRecommender, ProjectMemoryProvider, ProjectMemoryRequest,
        TurnMemoryExtractionRequest, TurnMemoryExtractor, TurnMemoryRecommendationRequest,
    },
    permissions::{AllowAllPermissionPolicy, PermissionPolicy, PermissionRequest},
    self_context::{AgentSelfHandle, AgentSelfState, SelfPromptAssembler},
    session::{SessionId, SessionMessage, SessionState, TurnId, UserMessage},
    skills::{SkillProvider, SkillSelectionRequest},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    truncation::ToolOutputTruncation,
    turn::{
        ExecutedToolCall, HarnessInferenceRequest, ModelClient, ResponseFormat, TokenSink,
        ToolChoice, TurnOutcome,
    },
    workspace::Workspace,
};
use codel00p_protocol::{
    ContextWindowState, CostEstimate, EventId, RuntimeErrorKind, SessionRole, TokenUsage,
};
use serde_json::json;

mod builder;
mod context;
mod turn;

pub use builder::AgentHarnessBuilder;

use context::{latest_user_message, summarize_compacted_messages};

const DEFAULT_COMPACTION_RECENT_MESSAGES: usize = 4;

pub struct AgentHarness {
    model_client: Arc<dyn ModelClient>,
    workspace: Workspace,
    tools: ToolRegistry,
    permission_policy: Arc<dyn PermissionPolicy>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
    lifecycle_hooks: Vec<Arc<dyn LifecycleHook>>,
    project_memory_provider: Option<Arc<dyn ProjectMemoryProvider>>,
    skill_provider: Option<Arc<dyn SkillProvider>>,
    turn_memory_extractor: Option<Arc<dyn TurnMemoryExtractor>>,
    memory_recommender: Option<Arc<dyn MemoryRecommender>>,
    memory_candidate_sink: Option<Arc<dyn MemoryCandidateSink>>,
    skill_extractor: Option<Arc<dyn SkillExtractor>>,
    skill_proposal_sink: Option<Arc<dyn SkillProposalSink>>,
    capability_extractor: Option<Arc<dyn crate::capability::CapabilityExtractor>>,
    capability_proposal_sink: Option<Arc<dyn crate::capability::CapabilityProposalSink>>,
    context_window: Option<ContextWindowState>,
    /// Shared self-awareness handle: the static identity/capabilities plus a live
    /// run-state snapshot the run loop refreshes each iteration. Read both to
    /// render the injected self block and by the `self_describe` tool. `None`
    /// disables self-awareness entirely.
    agent_self: Option<AgentSelfHandle>,
    /// The pre-rendered base operating prompt ("how I work"): rigor guidance plus
    /// (when `auto_plan` is on) planning guidance. Injected each turn after the
    /// self block and before project instructions. `None` injects nothing (back
    /// to pre-base-prompt behavior).
    base_prompt: Option<String>,
    /// Shared handle to the `update_plan` plan store, so the self-awareness
    /// run-state can report plan progress. `None` omits plan progress.
    plan_store: Option<crate::planning::PlanStore>,
    token_sink: Option<Arc<dyn TokenSink>>,
    max_iterations: u32,
    tool_output_truncation: ToolOutputTruncation,
    tool_choice: Option<ToolChoice>,
    response_format: Option<ResponseFormat>,
    cancel: CancelSignal,
}
