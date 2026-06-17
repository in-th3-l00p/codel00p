//! Candidate and extraction input types for memory workflows.

use codel00p_protocol::{
    MemoryEntry, MemoryEvidence, MemoryKind, MemorySensitivity, MemorySource, MemoryVisibility,
    ProjectRef,
};

use crate::{MemoryError, util::non_empty_filter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryCandidateInput {
    pub(crate) id: String,
    project: ProjectRef,
    kind: MemoryKind,
    sensitivity: MemorySensitivity,
    visibility: MemoryVisibility,
    content: String,
    source: MemorySource,
    tags: Vec<String>,
    evidence: Vec<MemoryEvidence>,
}

impl MemoryCandidateInput {
    pub fn new(
        id: impl Into<String>,
        project: ProjectRef,
        kind: MemoryKind,
        content: impl Into<String>,
        source: MemorySource,
    ) -> Self {
        Self {
            id: id.into(),
            project,
            kind,
            sensitivity: MemorySensitivity::Normal,
            visibility: MemoryVisibility::Project,
            content: content.into(),
            source,
            tags: Vec::new(),
            evidence: Vec::new(),
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        if let Some(tag) = non_empty_filter(tag.into()) {
            self.tags.push(tag);
        }
        self
    }

    pub fn with_evidence(mut self, evidence: MemoryEvidence) -> Self {
        self.evidence.push(evidence);
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

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn project(&self) -> &ProjectRef {
        &self.project
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

    pub fn source(&self) -> &MemorySource {
        &self.source
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn evidence(&self) -> &[MemoryEvidence] {
        &self.evidence
    }

    pub(crate) fn validate(&self) -> Result<(), MemoryError> {
        if self.id.trim().is_empty() {
            return Err(MemoryError::InvalidCandidate {
                message: "memory id cannot be empty".to_string(),
            });
        }

        if self.content.trim().is_empty() {
            return Err(MemoryError::InvalidCandidate {
                message: "memory content cannot be empty".to_string(),
            });
        }

        Ok(())
    }

    pub(crate) fn into_entry(self) -> MemoryEntry {
        let mut entry = MemoryEntry::new(self.id, self.project, self.kind, self.content)
            .with_source(self.source)
            .with_sensitivity(self.sensitivity)
            .with_visibility(self.visibility);
        for tag in self.tags {
            entry = entry.with_tag(tag);
        }
        for evidence in self.evidence {
            entry = entry.with_evidence(evidence);
        }
        entry
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryExtractionInput {
    project: ProjectRef,
    source: MemorySource,
    text: String,
    tags: Vec<String>,
}

impl MemoryExtractionInput {
    pub fn new(project: ProjectRef, source: MemorySource, text: impl Into<String>) -> Self {
        Self {
            project,
            source,
            text: text.into(),
            tags: Vec::new(),
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        if let Some(tag) = non_empty_filter(tag.into()) {
            self.tags.push(tag);
        }
        self
    }

    pub fn project(&self) -> &ProjectRef {
        &self.project
    }

    pub fn source(&self) -> &MemorySource {
        &self.source
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}
