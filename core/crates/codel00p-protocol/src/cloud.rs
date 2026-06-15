//! Cloud and team control-plane contracts for org-owned product data.

use serde::{Deserialize, Serialize};

use crate::{MemoryKind, MemorySensitivity, MemoryStatus};

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

/// A member of a Clerk organization, as returned by `GET /org/members`. codel00p
/// does not own membership; this is a read-only projection of Clerk's directory,
/// keyed by the Clerk `user_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgMember {
    user_id: String,
    role: OrgRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl OrgMember {
    pub fn new(user_id: impl Into<String>, role: OrgRole) -> Self {
        Self {
            user_id: user_id.into(),
            role,
            email: None,
            name: None,
        }
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = non_empty(email.into());
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = non_empty(name.into());
        self
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn role(&self) -> OrgRole {
        self.role
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
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
