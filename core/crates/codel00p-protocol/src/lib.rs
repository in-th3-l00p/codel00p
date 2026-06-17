//! Shared protocol contracts for codel00p crates and TypeScript fixtures.
//!
//! The crate root is a compatibility facade. Concrete contract families live in
//! focused modules so changes to sessions, tools, memory, providers, or cloud
//! control-plane types can be reviewed independently.

mod cloud;
mod context;
mod events;
mod ids;
mod memory;
mod permissions;
mod persistence;
mod provider;
mod runtime;
mod session;
mod tools;
mod version;

pub use cloud::{
    Agent, AgentUpdate, McpServer, McpServerUpdate, McpTransport, MemoryAuditEntry,
    MemoryReviewAction, NewAgent, NewMcpServer, NewMemoryCandidate, NewProject, OrgMember, OrgRef,
    OrgRole, Project, ProjectUpdate, Viewer,
};
pub use context::{CompactionRecord, ContextWindowState, ToolProgress};
pub use events::{AgentEvent, compute_manifest_hash};
pub use ids::{EventId, SessionId, TurnId};
pub use memory::{
    MemoryEntry, MemoryKind, MemorySensitivity, MemorySource, MemoryStatus, MemoryVisibility,
    ProjectRef,
};
pub use permissions::{
    PermissionDecision, PermissionMode, PermissionRequest, PermissionScope, PermissionStatus,
};
pub use persistence::SessionPersistenceEvent;
pub use provider::ProviderRef;
pub use runtime::RuntimeErrorKind;
pub use session::{SessionMessage, SessionRole};
pub use tools::{ToolCall, ToolResult};
pub use version::ProtocolVersion;
