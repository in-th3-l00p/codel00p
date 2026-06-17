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

/// Retrieval visibility scope for a memory, ordered narrow (`Private`) to wide
/// (`Org`). The ordering is meaningful: a max-visibility filter returns every
/// memory whose scope is at or below the requested width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryVisibility {
    Private,
    #[default]
    Project,
    Team,
    Org,
}

impl MemoryVisibility {
    pub fn is_project(&self) -> bool {
        *self == Self::Project
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

    /// Construct a source for a file import with no session/turn context.
    pub fn import(uri: impl Into<String>) -> Self {
        Self {
            session_id: SessionId::from_static(""),
            turn_id: TurnId::from_static(""),
            uri: Some(uri.into()),
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

    /// Returns true if this source was created via [`MemorySource::import`]
    /// (session_id and turn_id are empty sentinels, uri is set).
    pub fn is_import(&self) -> bool {
        self.session_id.as_str().is_empty() && self.uri.is_some()
    }
}

/// The category of an explicit evidence link backing a memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    File,
    Url,
    Pr,
    Issue,
    Commit,
    Other,
}

/// An explicit piece of source evidence backing a memory — a file path, URL,
/// PR/issue/commit reference, etc. — recorded alongside the session/turn replay
/// `MemorySource`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvidence {
    kind: EvidenceKind,
    reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

impl MemoryEvidence {
    pub fn new(kind: EvidenceKind, reference: impl Into<String>) -> Self {
        Self {
            kind,
            reference: reference.into(),
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        let note = note.into();
        self.note = if note.trim().is_empty() {
            None
        } else {
            Some(note)
        };
        self
    }

    pub fn kind(&self) -> EvidenceKind {
        self.kind
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
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
    #[serde(default, skip_serializing_if = "MemoryVisibility::is_project")]
    visibility: MemoryVisibility,
    content: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<MemorySource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence: Vec<MemoryEvidence>,
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
            visibility: MemoryVisibility::Project,
            content: content.into(),
            tags: Vec::new(),
            source: None,
            evidence: Vec::new(),
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

    pub fn with_visibility(mut self, visibility: MemoryVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_evidence(mut self, evidence: MemoryEvidence) -> Self {
        self.evidence.push(evidence);
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

    pub fn visibility(&self) -> MemoryVisibility {
        self.visibility
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

    pub fn evidence(&self) -> &[MemoryEvidence] {
        &self.evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_source_import_carries_uri_and_is_detectable() {
        let source = MemorySource::import("/abs/path/file.md");
        assert!(source.is_import());
        assert_eq!(source.uri(), Some("/abs/path/file.md"));
        assert!(source.session_id().as_str().is_empty());
        assert!(source.turn_id().as_str().is_empty());
    }

    #[test]
    fn memory_source_turn_is_not_import() {
        let source = MemorySource::turn(
            crate::SessionId::from_static("s-1"),
            crate::TurnId::from_static("t-1"),
        );
        assert!(!source.is_import());
    }
}
