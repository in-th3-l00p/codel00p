//! Query and filter builders for memory listing, retrieval, and quality review.

use codel00p_protocol::{
    MemoryKind, MemorySensitivity, MemoryStatus, MemoryVisibility, ProjectRef,
};

use crate::util::non_empty_filter;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryQuery {
    pub(crate) project: ProjectRef,
    pub(crate) kind: Option<MemoryKind>,
    pub(crate) sensitivity: Option<MemorySensitivity>,
    pub(crate) visibility: Option<MemoryVisibility>,
    pub(crate) tag: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryListFilter {
    pub(crate) project: ProjectRef,
    pub(crate) status: Option<MemoryStatus>,
    pub(crate) kind: Option<MemoryKind>,
    pub(crate) sensitivity: Option<MemorySensitivity>,
    pub(crate) visibility: Option<MemoryVisibility>,
    pub(crate) tag: Option<String>,
    pub(crate) limit: Option<usize>,
}

/// Free-text retrieval query: deterministic filters bound the candidate set,
/// then the query string is ranked against each memory's content by lexical
/// similarity. This is offline (no embeddings, no network).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRetrievalQuery {
    pub(crate) project: ProjectRef,
    pub(crate) query: String,
    pub(crate) kind: Option<MemoryKind>,
    pub(crate) sensitivity: Option<MemorySensitivity>,
    pub(crate) visibility: Option<MemoryVisibility>,
    pub(crate) tag: Option<String>,
    pub(crate) min_score: u8,
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySimilarityQuery {
    pub(crate) project: ProjectRef,
    pub(crate) kind: MemoryKind,
    pub(crate) content: String,
    pub(crate) min_score: u8,
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryStalenessQuery {
    pub(crate) project: ProjectRef,
    pub(crate) kind: Option<MemoryKind>,
    pub(crate) min_score: u8,
    pub(crate) limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryQualityQuery {
    pub(crate) project: ProjectRef,
    pub(crate) status: Option<MemoryStatus>,
    pub(crate) kind: Option<MemoryKind>,
    pub(crate) sensitivity: Option<MemorySensitivity>,
    pub(crate) tag: Option<String>,
    pub(crate) max_score: u8,
    pub(crate) limit: Option<usize>,
}

impl MemoryListFilter {
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            status: None,
            kind: None,
            sensitivity: None,
            visibility: None,
            tag: None,
            limit: None,
        }
    }

    pub fn with_status(mut self, status: MemoryStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }

    /// Restricts the listing to memories whose visibility is at or below the
    /// given scope (narrow→wide). Unset returns every visibility.
    pub fn with_visibility(mut self, visibility: MemoryVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = non_empty_filter(tag.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}

impl MemoryQualityQuery {
    /// Creates a review query for active memory with quality score 80 or lower.
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            status: None,
            kind: None,
            sensitivity: None,
            tag: None,
            max_score: 80,
            limit: None,
        }
    }

    /// Restricts the review queue to one active review status.
    pub fn with_status(mut self, status: MemoryStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Restricts the review queue to one memory kind.
    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Restricts the review queue to one memory sensitivity class.
    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }

    /// Restricts the review queue to memory records that carry a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = non_empty_filter(tag.into());
        self
    }

    /// Sets the inclusive maximum quality score returned by the review query.
    pub fn with_max_score(mut self, max_score: u8) -> Self {
        self.max_score = max_score;
        self
    }

    /// Limits the number of low-quality records returned.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}

impl MemoryStalenessQuery {
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            kind: None,
            min_score: 70,
            limit: None,
        }
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_min_score(mut self, min_score: u8) -> Self {
        self.min_score = min_score;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}

impl MemoryRetrievalQuery {
    /// Creates a ranked retrieval query for approved project memory. By default
    /// only memories sharing at least one token with the query (score >= 1) are
    /// returned, and sensitive memory is excluded.
    pub fn new(project: ProjectRef, query: impl Into<String>) -> Self {
        Self {
            project,
            query: query.into(),
            kind: None,
            sensitivity: None,
            visibility: None,
            tag: None,
            min_score: 1,
            limit: None,
        }
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }

    /// Restricts ranked retrieval to memories whose visibility is at or below
    /// the given scope (narrow→wide). Unset returns every visibility.
    pub fn with_visibility(mut self, visibility: MemoryVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = non_empty_filter(tag.into());
        self
    }

    /// Sets the inclusive minimum similarity score a memory must reach to be
    /// returned. A score of 0 returns every filtered candidate.
    pub fn with_min_score(mut self, min_score: u8) -> Self {
        self.min_score = min_score;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}

impl MemorySimilarityQuery {
    pub fn new(project: ProjectRef, kind: MemoryKind, content: impl Into<String>) -> Self {
        Self {
            project,
            kind,
            content: content.into(),
            min_score: 70,
            limit: None,
        }
    }

    pub fn with_min_score(mut self, min_score: u8) -> Self {
        self.min_score = min_score;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}

impl MemoryQuery {
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            kind: None,
            sensitivity: None,
            visibility: None,
            tag: None,
            text: None,
            limit: None,
        }
    }

    pub fn with_kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }

    /// Restricts retrieval to memories whose visibility is at or below the given
    /// scope (narrow→wide). Unset returns every visibility.
    pub fn with_visibility(mut self, visibility: MemoryVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = non_empty_filter(tag.into());
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = non_empty_filter(text.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }
}
