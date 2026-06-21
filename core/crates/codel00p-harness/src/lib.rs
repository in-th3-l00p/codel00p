pub mod agent;
pub mod background;
pub mod base_prompt;
pub mod cancel;
pub mod capability;
pub mod checkpoints;
pub mod checks;
pub mod code_exec;
pub mod commands;
pub mod context;
pub mod delegation;
pub mod editing;
pub mod errors;
pub mod event_sink;
pub mod events;
pub mod find;
pub mod git;
pub mod instructions;
pub mod iteration_budget;
pub mod learning;
pub mod lifecycle;
pub mod memory;
pub mod permissions;
pub mod pipeline;
pub mod planning;
pub mod pr;
pub mod provider_adapter;
pub mod repo_map;
pub mod self_context;
pub mod session;
pub mod skills;
mod streaming;
pub mod subagent;
pub mod terminal;
pub mod tool_registry;
pub mod tool_result;
pub mod tools;
pub mod truncation;
pub mod turn;
mod validation;
mod walk;
pub mod web;
pub mod workspace;

pub use agent::{AgentHarness, AgentHarnessBuilder, VerifyConfig};
pub use background::BackgroundProcesses;
pub use base_prompt::base_prompt;
pub use cancel::CancelSignal;
pub use capability::{
    Capability, CapabilityCandidateCall, CapabilityExtractionRequest, CapabilityExtractor,
    CapabilityProposalSink, CapabilityTool, FileCapabilityProposalSink, ModelCapabilityExtractor,
    PipelineCapabilityExtractor, ProposeCapabilityTool, VerificationOutcome, capability_tools,
    load_capabilities, verify_capability,
};
pub use checkpoints::{
    Checkpoint, CheckpointStore, CreateCheckpointTool, ListCheckpointsTool, RestoreCheckpointTool,
    RestoreMode, RestoreOutcome, checkpoint_tools,
};
pub use checks::{CheckSummary, DetectedChecks, RunChecksTool, detect_checks, parse_summary};
pub use code_exec::{CodeExecutionEngine, ExecuteCodeTool, code_execution_tools};
pub use codel00p_protocol::{ContextWindowState, RuntimeErrorKind};
pub use commands::{ProcessKillTool, ProcessListTool, ProcessOutputTool, RunCommandTool};
pub use delegation::{
    AgentRole, DelegateTaskTool, DelegatedTask, DelegationOutcome, SubAgentSpawner, TaskIsolation,
    delegation_tools,
};
pub use errors::HarnessError;
pub use event_sink::AgentEventSink;
pub use events::HarnessEvent;
pub use find::{FindFilesTool, GrepTool};
pub use instructions::{ProjectInstruction, ProjectInstructionLoader, ProjectInstructions};
pub use iteration_budget::IterationBudget;
pub use learning::{
    ProcedureSkillExtractor, ProposeSkillTool, ProposedSkill, SkillExtractionRequest,
    SkillExtractor, SkillProposalSink, learning_tools,
};
pub use lifecycle::{LifecycleHook, TurnLifecycleContext};
pub use memory::{
    DeterministicMemoryRecommender, ExplicitTurnMemoryExtractor, MemoryCandidateSink,
    MemoryCandidateSinkOutcome, MemoryPromptAssembler, MemoryRecommender,
    MemoryRepositoryCandidateSink, MemoryRepositoryProjectMemoryProvider, ProjectMemoryContext,
    ProjectMemoryItem, ProjectMemoryProvider, ProjectMemoryRequest, RecommendationToolCall,
    TurnMemoryExtractionRequest, TurnMemoryExtractor, TurnMemoryRecommendationRequest,
};
pub use permissions::{
    AllowAllPermissionPolicy, PermissionDecision, PermissionMode, PermissionPolicy,
    PermissionRequest, PermissionScope,
};
pub use pipeline::{
    DispatchOutcome, PipelineEngine, PipelineRun, PipelineStep, RunPipelineTool, dispatch_tool,
    parse_steps, pipeline_tools,
};
pub use planning::{PlanItem, PlanStatus, PlanStore, UpdatePlanTool};
pub use pr::PreparePrTool;
pub use provider_adapter::ProviderModelClient;
pub use repo_map::RepoMapTool;
pub use self_context::{
    AgentSelfContext, AgentSelfHandle, AgentSelfState, SelfDescribeTool, SelfPromptAssembler,
};
pub use session::{
    SessionCompactionRecord, SessionId, SessionMessage, SessionState, TurnId, UserMessage,
};
pub use skills::{
    SkillContext, SkillPrompt, SkillPromptAssembler, SkillProvider, SkillSelectionRequest,
};
pub use subagent::HarnessSubAgentSpawner;
pub use terminal::{
    ChildHandle, CommandOutcome, CommandSpec, DirEntry, DockerBackend, DockerConfig, FileKind,
    LocalBackend, OutputLimits, SshBackend, SshConfig, TerminalBackend,
};
pub use tool_registry::{TOOL_DESCRIBE, TOOL_SEARCH, ToolRegistry};
pub use tool_result::ToolResult;
pub use tools::{Tool, ToolSpec};
pub use turn::{
    ExecutedToolCall, HarnessInferenceRequest, HarnessInferenceResponse, ModelClient,
    ModelToolCall, ResponseFormat, TokenSink, ToolChoice, TurnOutcome,
};
pub use web::{WebFetchTool, WebSearchTool, web_tools};
pub use workspace::Workspace;

pub fn crate_name() -> &'static str {
    "codel00p-harness"
}
