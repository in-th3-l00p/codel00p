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

pub use errors::HarnessError;
pub use events::HarnessEvent;
pub use session::{SessionId, SessionMessage, SessionState, TurnId, UserMessage};
pub use workspace::Workspace;

pub fn crate_name() -> &'static str {
    "codel00p-harness"
}
