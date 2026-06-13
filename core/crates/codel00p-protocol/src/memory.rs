//! Reviewed project-memory contracts shared by CLI, cloud, and storage.

use serde::{Deserialize, Serialize};

use crate::{SessionId, TurnId};

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
