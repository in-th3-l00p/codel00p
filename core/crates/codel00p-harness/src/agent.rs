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
    /// Whether to inject the live "Workspace state" block each turn (git status,
    /// detected test/build/lint commands, files edited this turn). Default off at
    /// the harness layer (exactly today's behavior); the CLI opts users in with an
    /// on-by-default `[agent.behavior]` toggle.
    workspace_context: bool,
    /// Shared handle to the `update_plan` plan store, so the self-awareness
    /// run-state can report plan progress. `None` omits plan progress.
    plan_store: Option<crate::planning::PlanStore>,
    token_sink: Option<Arc<dyn TokenSink>>,
    max_iterations: u32,
    tool_output_truncation: ToolOutputTruncation,
    tool_choice: Option<ToolChoice>,
    response_format: Option<ResponseFormat>,
    cancel: CancelSignal,
    /// Verify-before-done + self-critique configuration. Defaults are "today's
    /// behavior off" so a harness with no explicit config behaves exactly as
    /// before; the CLI wires the `[agent.behavior]` toggles onto it.
    verify: VerifyConfig,
    /// In-turn error self-correction configuration (classification hints +
    /// repeated-failure budget / replan nudge). Default is "today's behavior" so
    /// an unconfigured harness emits bare errors and never nudges; the CLI wires
    /// the `[agent.behavior]` toggles onto it.
    self_correct: SelfCorrectConfig,
}

/// Configuration for in-turn error self-correction (#12 T0.4).
///
/// Two independent levers, both toggleable:
///
/// * [`error_hints`](Self::error_hints) — when a tool call fails (an `Err`, or a
///   result whose `error`/non-zero exit signals failure), classify the failure
///   via [`crate::error_classify::classify`] and enrich the error payload fed
///   back to the model with `error_kind` + an actionable `hint`. Off ⇒ bare
///   `{ "error": ... }` exactly as before.
/// * [`replan_on_failure`](Self::replan_on_failure) — track consecutive failures
///   of the *same operation* (tool name + program/args for command tools, else
///   the tool name) within a turn; when one operation fails
///   [`failure_budget`](Self::failure_budget) times in a row, inject a stronger
///   "step back and reconsider / replan" nudge (a user message) so the agent
///   stops looping on the same broken call. The nudge never aborts the turn —
///   the iteration budget still bounds it.
///
/// The default is **off / disabled** at the harness layer so an unconfigured
/// harness behaves exactly as before; the CLI defaults the user-facing toggles
/// to on.
#[derive(Clone, Debug)]
pub struct SelfCorrectConfig {
    /// Attach classification (`error_kind`) + `hint` to failed tool results.
    pub error_hints: bool,
    /// Emit the step-back/replan nudge when the failure budget is hit.
    pub replan_on_failure: bool,
    /// Consecutive same-operation failures before the replan nudge fires. A
    /// value of 0 disables the budget entirely (no nudge ever).
    pub failure_budget: u32,
}

impl Default for SelfCorrectConfig {
    fn default() -> Self {
        // Off at the harness layer: bare errors, no nudge — exactly today's
        // behavior. The CLI opts users in with on-by-default toggles.
        Self {
            error_hints: false,
            replan_on_failure: false,
            failure_budget: 3,
        }
    }
}

/// Configuration for the verify-before-done loop and the self-critique step.
///
/// The done-point of a turn (model emits no tool calls) is the hook: when
/// [`self_verify`](Self::self_verify) is on and the turn made **mutating**
/// changes, the harness runs the project's checks via the registered
/// `run_checks` tool *before* completing. On failure it feeds the failure back
/// into the conversation and keeps looping (bounded by
/// [`verify_iterations`](Self::verify_iterations)); on pass (or when there is
/// nothing to verify) it proceeds. When [`self_critique`](Self::self_critique)
/// is on, the model then gets one reflection turn before final completion.
///
/// The default is **off** (`Default`) so an unconfigured harness behaves exactly
/// as it did before this feature; the CLI defaults the user-facing toggles to on.
#[derive(Clone, Debug)]
pub struct VerifyConfig {
    /// Master switch for the verify-before-done phase.
    pub self_verify: bool,
    /// Run the `test` check during verification.
    pub auto_test: bool,
    /// Also run `lint` and feed failures back (opt-in; lint can be noisy).
    pub lint_and_fix: bool,
    /// Run the metacognition / self-critique reflection step before completion.
    pub self_critique: bool,
    /// Max verify→fix attempts before completing with a not-verified signal.
    pub verify_iterations: u32,
    /// Explicit command override passed to `run_checks` instead of detection.
    pub test_command: Option<String>,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        // Off by default at the harness layer: an unconfigured harness keeps the
        // pre-feature behavior (complete immediately on no-tool-calls). The CLI
        // opts users in with on-by-default `[agent.behavior]` toggles.
        Self {
            self_verify: false,
            auto_test: true,
            lint_and_fix: false,
            self_critique: false,
            verify_iterations: 3,
            test_command: None,
        }
    }
}
