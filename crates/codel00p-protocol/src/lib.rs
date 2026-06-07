use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
static TURN_COUNTER: AtomicU64 = AtomicU64::new(1);
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(1);

const CURRENT_PROTOCOL_VERSION: &str = "codel00p.protocol.v1";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(String);

impl ProtocolVersion {
    pub fn current() -> Self {
        Self(CURRENT_PROTOCOL_VERSION.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! id_type {
    ($name:ident, $prefix:literal, $counter:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                let id = $counter.fetch_add(1, Ordering::Relaxed);
                Self(format!("{}-{id}", $prefix))
            }

            pub fn from_static(value: &'static str) -> Self {
                Self(value.to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(SessionId, "session", SESSION_COUNTER);
id_type!(TurnId, "turn", TURN_COUNTER);
id_type!(EventId, "event", EVENT_COUNTER);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionMessage {
    role: SessionRole,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

impl SessionMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(SessionRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text(SessionRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(SessionRole::Assistant, content)
    }

    pub fn tool(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            role: SessionRole::Tool,
            content: payload.to_string(),
            tool_call_id: Some(tool_call_id.into()),
            tool_name: Some(tool_name.into()),
            payload: Some(payload),
        }
    }

    pub fn role(&self) -> SessionRole {
        self.role
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }

    fn text(role: SessionRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
            payload: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    id: String,
    name: String,
    input: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input(&self) -> &Value {
        &self.input
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    call_id: String,
    name: String,
    output: Value,
    success: bool,
}

impl ToolResult {
    pub fn success(call_id: impl Into<String>, name: impl Into<String>, output: Value) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            output,
            success: true,
        }
    }

    pub fn failure(
        call_id: impl Into<String>,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            output: serde_json::json!({ "error": message.into() }),
            success: false,
        }
    }

    pub fn output(&self) -> &Value {
        &self.output
    }

    pub fn success_status(&self) -> bool {
        self.success
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        event_id: EventId,
        session_id: SessionId,
    },
    TurnStarted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
    },
    ContextBuilt {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        message_count: usize,
    },
    InferenceRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        provider: String,
        model: String,
    },
    InferenceCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        finish_reason: Option<String>,
    },
    ToolCallRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
    },
    ToolCallCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
    },
    ToolCallFailed {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        message: String,
    },
    TurnCompleted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        iterations: u32,
    },
}

impl AgentEvent {
    pub fn tool_call_completed(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
    ) -> Self {
        Self::ToolCallCompleted {
            event_id,
            session_id,
            turn_id,
            tool_name: tool_name.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRef {
    id: String,
    name: String,
}

impl ProjectRef {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Architecture,
    Convention,
    Workflow,
    Decision,
    Deployment,
    Troubleshooting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Candidate,
    Approved,
    Rejected,
    Archived,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySource {
    session_id: SessionId,
    turn_id: TurnId,
}

impl MemorySource {
    pub fn turn(session_id: SessionId, turn_id: TurnId) -> Self {
        Self {
            session_id,
            turn_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    id: String,
    project: ProjectRef,
    kind: MemoryKind,
    status: MemoryStatus,
    content: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<MemorySource>,
}

impl MemoryEntry {
    pub fn new(
        id: impl Into<String>,
        project: ProjectRef,
        kind: MemoryKind,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            project,
            kind,
            status: MemoryStatus::Candidate,
            content: content.into(),
            tags: Vec::new(),
            source: None,
        }
    }

    pub fn with_status(mut self, status: MemoryStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_source(mut self, source: MemorySource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn project(&self) -> &ProjectRef {
        &self.project
    }

    pub fn status(&self) -> MemoryStatus {
        self.status
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRef {
    provider: String,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
}

impl ProviderRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            alias: None,
            base_url: None,
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}
