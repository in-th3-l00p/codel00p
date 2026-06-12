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
    ContextCompacted {
        event_id: EventId,
        session_id: SessionId,
        turn_id: TurnId,
        before_message_count: usize,
        after_message_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySensitivity {
    #[default]
    Normal,
    Sensitive,
}

impl MemorySensitivity {
    pub fn is_normal(&self) -> bool {
        *self == Self::Normal
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySource {
    session_id: SessionId,
    turn_id: TurnId,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<String>,
}

impl MemorySource {
    pub fn turn(session_id: SessionId, turn_id: TurnId) -> Self {
        Self {
            session_id,
            turn_id,
            uri: None,
        }
    }

    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        let uri = uri.into();
        self.uri = if uri.trim().is_empty() {
            None
        } else {
            Some(uri)
        };
        self
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    id: String,
    project: ProjectRef,
    kind: MemoryKind,
    status: MemoryStatus,
    #[serde(default, skip_serializing_if = "MemorySensitivity::is_normal")]
    sensitivity: MemorySensitivity,
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
            sensitivity: MemorySensitivity::Normal,
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

    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = sensitivity;
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

    pub fn sensitivity(&self) -> MemorySensitivity {
        self.sensitivity
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

// --- Cloud / team control-plane contracts ---
//
// codel00p does not own the identity or membership model. Clerk is the source
// of truth for organizations, members, roles, and invitations; the cloud
// service keys its product data (projects, policy, memory, audit) by these
// Clerk identifiers.

/// A reference to a Clerk organization — the unit of team ownership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgRef {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
}

impl OrgRef {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            slug: None,
        }
    }

    pub fn with_slug(mut self, slug: impl Into<String>) -> Self {
        let slug = slug.into();
        self.slug = if slug.trim().is_empty() {
            None
        } else {
            Some(slug)
        };
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn slug(&self) -> Option<&str> {
        self.slug.as_deref()
    }
}

/// An organization role, normalized from Clerk's `org:admin` / `org:member`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    Admin,
    Member,
}

impl OrgRole {
    /// Parses Clerk's organization role claim (`org:admin`, `org:member`) or a
    /// bare `admin` / `member` token.
    pub fn from_clerk_claim(claim: &str) -> Option<Self> {
        match claim.trim() {
            "org:admin" | "admin" => Some(Self::Admin),
            "org:member" | "member" => Some(Self::Member),
            _ => None,
        }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

/// The authenticated caller, resolved from a verified Clerk session token. This
/// is the shape returned by `GET /me`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    org: Option<OrgRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_role: Option<OrgRole>,
}

impl Viewer {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            email: None,
            org: None,
            org_role: None,
        }
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        let email = email.into();
        self.email = if email.trim().is_empty() {
            None
        } else {
            Some(email)
        };
        self
    }

    pub fn with_org(mut self, org: OrgRef, role: OrgRole) -> Self {
        self.org = Some(org);
        self.org_role = Some(role);
        self
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn org(&self) -> Option<&OrgRef> {
        self.org.as_ref()
    }

    pub fn org_role(&self) -> Option<OrgRole> {
        self.org_role
    }
}

/// A project owned by an organization. Product data, keyed by the Clerk org id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    id: String,
    org_id: String,
    name: String,
    slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository_url: Option<String>,
}

impl Project {
    pub fn new(
        id: impl Into<String>,
        org_id: impl Into<String>,
        name: impl Into<String>,
        slug: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            org_id: org_id.into(),
            name: name.into(),
            slug: slug.into(),
            repository_url: None,
        }
    }

    pub fn with_repository_url(mut self, repository_url: impl Into<String>) -> Self {
        let repository_url = repository_url.into();
        self.repository_url = if repository_url.trim().is_empty() {
            None
        } else {
            Some(repository_url)
        };
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn org_id(&self) -> &str {
        &self.org_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }

    pub fn repository_url(&self) -> Option<&str> {
        self.repository_url.as_deref()
    }
}

/// The request body for creating a project. The owning organization comes from
/// the caller's verified session, never the request payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

impl NewProject {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            repository_url: None,
        }
    }
}

/// A request to contribute a memory candidate to a project's review queue,
/// typically pushed from a CLI or desktop workspace after useful agent work.
/// The owning org/project come from the route and session; the server forces
/// the new entry's status to `candidate`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMemoryCandidate {
    pub kind: MemoryKind,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sensitivity: MemorySensitivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
}

impl NewMemoryCandidate {
    pub fn new(kind: MemoryKind, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
            tags: Vec::new(),
            sensitivity: MemorySensitivity::Normal,
            source_uri: None,
        }
    }
}

/// An action recorded against a memory entry in its review history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReviewAction {
    Created,
    Approved,
    Rejected,
    Archived,
}

impl MemoryReviewAction {
    /// The memory status this action transitions an entry into, if any.
    pub fn resulting_status(self) -> Option<MemoryStatus> {
        match self {
            Self::Created => Some(MemoryStatus::Candidate),
            Self::Approved => Some(MemoryStatus::Approved),
            Self::Rejected => Some(MemoryStatus::Rejected),
            Self::Archived => Some(MemoryStatus::Archived),
        }
    }
}

/// One entry in a memory review audit trail: who did what to which memory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAuditEntry {
    pub memory_id: String,
    pub action: MemoryReviewAction,
    pub actor: String,
}

impl MemoryAuditEntry {
    pub fn new(
        memory_id: impl Into<String>,
        action: MemoryReviewAction,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            memory_id: memory_id.into(),
            action,
            actor: actor.into(),
        }
    }
}

/// A partial update to a project. Absent fields are left unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// Transport an MCP server speaks: a local stdio process or a remote HTTP server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Http,
}

/// A team MCP server configuration — the org-shared pool of external tools that
/// agents can connect to. Product data, project-scoped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    id: String,
    org_id: String,
    project_id: String,
    name: String,
    transport: McpTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    enabled: bool,
    created_by: String,
}

impl McpServer {
    pub fn new(
        id: impl Into<String>,
        org_id: impl Into<String>,
        project_id: impl Into<String>,
        name: impl Into<String>,
        transport: McpTransport,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            org_id: org_id.into(),
            project_id: project_id.into(),
            name: name.into(),
            transport,
            command: None,
            url: None,
            enabled: true,
            created_by: created_by.into(),
        }
    }

    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = non_empty(command.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = non_empty(url.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn org_id(&self) -> &str {
        &self.org_id
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn transport(&self) -> McpTransport {
        self.transport
    }

    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn created_by(&self) -> &str {
        &self.created_by
    }
}

/// Request to create an MCP server. The owning org/project come from the route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMcpServer {
    pub name: String,
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Partial update to an MCP server. Absent fields are left unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<McpTransport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// A stored agent definition — the org-shared pool of agents a team can run.
/// Product data, project-scoped, referencing MCP servers by id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    id: String,
    org_id: String,
    project_id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    provider: String,
    model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    mcp_server_ids: Vec<String>,
    created_by: String,
}

impl Agent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        org_id: impl Into<String>,
        project_id: impl Into<String>,
        name: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            org_id: org_id.into(),
            project_id: project_id.into(),
            name: name.into(),
            description: None,
            instructions: None,
            provider: provider.into(),
            model: model.into(),
            mcp_server_ids: Vec::new(),
            created_by: created_by.into(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = non_empty(description.into());
        self
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = non_empty(instructions.into());
        self
    }

    pub fn with_mcp_server_ids(mut self, ids: Vec<String>) -> Self {
        self.mcp_server_ids = ids;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn org_id(&self) -> &str {
        &self.org_id
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn mcp_server_ids(&self) -> &[String] {
        &self.mcp_server_ids
    }

    pub fn created_by(&self) -> &str {
        &self.created_by
    }
}

/// Request to create an agent. The owning org/project come from the route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewAgent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
}

/// Partial update to an agent. Absent fields are left unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_server_ids: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

fn non_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
