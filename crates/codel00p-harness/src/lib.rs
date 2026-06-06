pub mod agent;
pub mod context;
pub mod errors;
pub mod events;
pub mod session;
pub mod tool_registry;
pub mod tool_result;
pub mod tools;
pub mod turn;
pub mod workspace;

pub use agent::{AgentHarness, AgentHarnessBuilder};
pub use errors::HarnessError;
pub use events::HarnessEvent;
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
