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
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
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

    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: SessionRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_name: None,
            payload: None,
            tool_calls,
        }
    }

    pub fn tool(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        payload: Value,
    ) -> Self {
        let content = payload.to_string();
        Self::tool_result_parts(tool_call_id, tool_name, content, Some(payload))
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::tool_result_parts(tool_call_id, tool_name, content, None)
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        self.tool_call_id.as_deref()
    }

    pub fn tool_name(&self) -> Option<&str> {
        self.tool_name.as_deref()
    }

    fn tool_result_parts(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        content: impl Into<String>,
        payload: Option<Value>,
    ) -> Self {
        Self {
            role: SessionRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_name: Some(tool_name.into()),
            payload,
            tool_calls: Vec::new(),
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

    pub fn tool_calls(&self) -> &[ToolCall] {
        &self.tool_calls
    }

    fn text(role: SessionRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
            payload: None,
            tool_calls: Vec::new(),
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
    PermissionRequested {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        request_id: String,
        scope: PermissionScope,
    },
    PermissionDenied {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        request_id: String,
        message: String,
    },
    ToolProgress {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: String,
        phase: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    LifecycleHookFailed {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        hook: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeErrorKind {
    ProviderAuth,
    ProviderRateLimit,
    ProviderBilling,
    ProviderUnavailable,
    ModelUnavailable,
    ContextOverflow,
    PayloadTooLarge,
    PermissionDenied,
    ToolExecution,
    InvalidToolInput,
    Cancelled,
    IterationLimit,
    Storage,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    ReadOnly,
    WorkspaceWrite,
    Shell,
    Network,
    ExternalConnector,
    MemoryWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    request_id: String,
    session_id: SessionId,
    turn_id: TurnId,
    tool_name: String,
    input: Value,
    scope: PermissionScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl PermissionRequest {
    pub fn new(
        request_id: impl Into<String>,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
        input: Value,
        scope: PermissionScope,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            session_id,
            turn_id,
            tool_name: tool_name.into(),
            input,
            scope,
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn id(&self) -> &str {
        &self.request_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn input(&self) -> &Value {
        &self.input
    }

    pub fn scope(&self) -> PermissionScope {
        self.scope
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDecision {
    request_id: String,
    mode: PermissionMode,
    status: PermissionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl PermissionDecision {
    pub fn allow(request_id: impl Into<String>, mode: PermissionMode) -> Self {
        Self {
            request_id: request_id.into(),
            mode,
            status: PermissionStatus::Allow,
            message: None,
        }
    }

    pub fn deny(
        request_id: impl Into<String>,
        mode: PermissionMode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            mode,
            status: PermissionStatus::Deny,
            message: Some(message.into()),
        }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    pub fn status(&self) -> PermissionStatus {
        self.status
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn allows_execution(&self) -> bool {
        self.status == PermissionStatus::Allow
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextWindowState {
    model: String,
    context_limit_tokens: u64,
    used_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning_threshold_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocking_threshold_tokens: Option<u64>,
}

impl ContextWindowState {
    pub fn new(model: impl Into<String>, context_limit_tokens: u64, used_tokens: u64) -> Self {
        Self {
            model: model.into(),
            context_limit_tokens,
            used_tokens,
            warning_threshold_tokens: None,
            blocking_threshold_tokens: None,
        }
    }

    pub fn with_warning_threshold(mut self, tokens: u64) -> Self {
        self.warning_threshold_tokens = Some(tokens);
        self
    }

    pub fn with_blocking_threshold(mut self, tokens: u64) -> Self {
        self.blocking_threshold_tokens = Some(tokens);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn context_limit_tokens(&self) -> u64 {
        self.context_limit_tokens
    }

    pub fn used_tokens(&self) -> u64 {
        self.used_tokens
    }

    pub fn is_above_warning_threshold(&self) -> bool {
        self.warning_threshold_tokens
            .map(|threshold| self.used_tokens >= threshold)
            .unwrap_or(false)
    }

    pub fn is_at_blocking_limit(&self) -> bool {
        self.blocking_threshold_tokens
            .map(|threshold| self.used_tokens >= threshold)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionRecord {
    event_id: EventId,
    session_id: SessionId,
    turn_id: TurnId,
    before_message_count: usize,
    after_message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

impl CompactionRecord {
    pub fn new(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        before_message_count: usize,
        after_message_count: usize,
    ) -> Self {
        Self {
            event_id,
            session_id,
            turn_id,
            before_message_count,
            after_message_count,
            summary: None,
        }
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn before_message_count(&self) -> usize {
        self.before_message_count
    }

    pub fn after_message_count(&self) -> usize {
        self.after_message_count
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProgress {
    event_id: EventId,
    session_id: SessionId,
    turn_id: TurnId,
    tool_name: String,
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl ToolProgress {
    pub fn new(
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
        phase: impl Into<String>,
    ) -> Self {
        Self {
            event_id,
            session_id,
            turn_id,
            tool_name: tool_name.into(),
            phase: phase.into(),
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn phase(&self) -> &str {
        &self.phase
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionPersistenceEvent {
    RecordAppended {
        event_id: EventId,
        session_id: SessionId,
        record_id: String,
        sequence: u64,
    },
    ReplayStarted {
        event_id: EventId,
        session_id: SessionId,
    },
    ReplayCompleted {
        event_id: EventId,
        session_id: SessionId,
        record_count: u64,
    },
}

impl SessionPersistenceEvent {
    pub fn record_appended(
        event_id: EventId,
        session_id: SessionId,
        record_id: impl Into<String>,
        sequence: u64,
    ) -> Self {
        Self::RecordAppended {
            event_id,
            session_id,
            record_id: record_id.into(),
            sequence,
        }
    }

    pub fn record_id(&self) -> &str {
        match self {
            Self::RecordAppended { record_id, .. } => record_id,
            Self::ReplayStarted { .. } | Self::ReplayCompleted { .. } => "",
        }
    }

    pub fn sequence(&self) -> u64 {
        match self {
            Self::RecordAppended { sequence, .. } => *sequence,
            Self::ReplayStarted { .. } | Self::ReplayCompleted { .. } => 0,
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

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
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

    pub fn kind(&self) -> MemoryKind {
        self.kind
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn source(&self) -> Option<&MemorySource> {
        self.source.as_ref()
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
