pub mod agent;
pub mod context;
pub mod errors;
pub mod events;
pub mod permissions;
pub mod provider_adapter;
pub mod session;
pub mod tool_registry;
pub mod tool_result;
pub mod tools;
pub mod turn;
pub mod workspace;

pub use agent::{AgentHarness, AgentHarnessBuilder};
pub use codel00p_protocol::{ContextWindowState, RuntimeErrorKind};
pub use errors::HarnessError;
pub use events::HarnessEvent;
pub use permissions::{
    AllowAllPermissionPolicy, PermissionDecision, PermissionMode, PermissionPolicy,
    PermissionRequest, PermissionScope,
};
pub use provider_adapter::ProviderModelClient;
pub use session::{SessionId, SessionMessage, SessionState, TurnId, UserMessage};
pub use tool_registry::ToolRegistry;
pub use tool_result::ToolResult;
pub use tools::Tool;
pub use turn::{
    ExecutedToolCall, HarnessInferenceRequest, HarnessInferenceResponse, ModelClient,
    ModelToolCall, TurnOutcome,
};
pub use workspace::Workspace;

pub fn crate_name() -> &'static str {
    "codel00p-harness"
}
