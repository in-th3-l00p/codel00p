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
        MemoryCandidateSink, ProjectMemoryProvider, ProjectMemoryRequest,
        TurnMemoryExtractionRequest, TurnMemoryExtractor,
    },
    permissions::{AllowAllPermissionPolicy, PermissionPolicy, PermissionRequest},
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
use codel00p_protocol::{ContextWindowState, EventId, RuntimeErrorKind, SessionRole};
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
    memory_candidate_sink: Option<Arc<dyn MemoryCandidateSink>>,
    skill_extractor: Option<Arc<dyn SkillExtractor>>,
    skill_proposal_sink: Option<Arc<dyn SkillProposalSink>>,
    capability_extractor: Option<Arc<dyn crate::capability::CapabilityExtractor>>,
    capability_proposal_sink: Option<Arc<dyn crate::capability::CapabilityProposalSink>>,
    context_window: Option<ContextWindowState>,
    token_sink: Option<Arc<dyn TokenSink>>,
    max_iterations: u32,
    tool_output_truncation: ToolOutputTruncation,
    tool_choice: Option<ToolChoice>,
    response_format: Option<ResponseFormat>,
    cancel: CancelSignal,
}
