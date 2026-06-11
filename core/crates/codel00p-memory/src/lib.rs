use std::collections::BTreeSet;

use codel00p_protocol::{
    MemoryEntry, MemoryKind, MemorySensitivity, MemorySource, MemoryStatus, ProjectRef,
};
use codel00p_storage::{
    AppendLogEntry, AppendLogStore, DocumentStore, InMemoryStorage, StorageDocument, StorageError,
    StorageScope,
};
use serde::{Deserialize, Serialize};

const MEMORY_COLLECTION: &str = "memory_entries";
const MEMORY_AUDIT_STREAM_PREFIX: &str = "memory";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryCandidateInput {
    id: String,
    project: ProjectRef,
    kind: MemoryKind,
    sensitivity: MemorySensitivity,
    content: String,
    source: MemorySource,
    tags: Vec<String>,
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
            content: content.into(),
            source,
            tags: Vec::new(),
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        if let Some(tag) = non_empty_filter(tag.into()) {
            self.tags.push(tag);
        }
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: MemorySensitivity) -> Self {
        self.sensitivity = sensitivity;
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

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn source(&self) -> &MemorySource {
        &self.source
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    fn validate(&self) -> Result<(), MemoryError> {
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

    fn into_entry(self) -> MemoryEntry {
        let mut entry = MemoryEntry::new(self.id, self.project, self.kind, self.content)
            .with_source(self.source)
            .with_sensitivity(self.sensitivity);
        for tag in self.tags {
            entry = entry.with_tag(tag);
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

pub trait MemoryCandidateExtractor {
    fn extract(
        &self,
        input: MemoryExtractionInput,
    ) -> Result<Vec<MemoryCandidateInput>, MemoryError>;
}

#[derive(Clone, Debug, Default)]
pub struct ExplicitMemoryExtractor;

impl MemoryCandidateExtractor for ExplicitMemoryExtractor {
    fn extract(
        &self,
        input: MemoryExtractionInput,
    ) -> Result<Vec<MemoryCandidateInput>, MemoryError> {
        let mut candidates = Vec::new();
        for line in input.text().lines() {
            let Some((kind, directive_tags, content)) = parse_remember_directive(line) else {
                continue;
            };

            let id = format!(
                "memory-candidate-{}-{}-{}",
                input.source().session_id().as_str(),
                input.source().turn_id().as_str(),
                candidates.len() + 1
            );
            let mut candidate = MemoryCandidateInput::new(
                id,
                input.project().clone(),
                kind,
                content,
                input.source().clone(),
            );
            for tag in input.tags() {
                candidate = candidate.with_tag(tag);
            }
            for tag in directive_tags {
                candidate = candidate.with_tag(tag);
            }
            candidates.push(candidate);
        }

        Ok(candidates)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    entry: MemoryEntry,
}

impl MemoryRecord {
    fn new(entry: MemoryEntry) -> Self {
        Self { entry }
    }

    pub fn entry(&self) -> &MemoryEntry {
        &self.entry
    }

    /// Returns deterministic advisory quality signals for review workflows.
    pub fn quality(&self) -> MemoryQuality {
        score_memory_entry(&self.entry)
    }
}

/// Advisory quality score for a memory record.
///
/// Quality findings help review surfaces prioritize cleanup, but they do not
/// change lifecycle state, retrieval eligibility, or duplicate detection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuality {
    score: u8,
    findings: Vec<String>,
}

impl MemoryQuality {
    /// A deterministic score from 0 to 100, where higher is more reusable.
    pub fn score(&self) -> u8 {
        self.score
    }

    /// Stable human-readable findings explaining score deductions.
    pub fn findings(&self) -> &[String] {
        &self.findings
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewDecision {
    Approve { actor: String },
    Reject { actor: String, reason: String },
    Archive { actor: String, reason: String },
}

impl ReviewDecision {
    pub fn approve(actor: impl Into<String>) -> Self {
        Self::Approve {
            actor: actor.into(),
        }
    }

    pub fn reject(actor: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Reject {
            actor: actor.into(),
            reason: reason.into(),
        }
    }

    pub fn archive(actor: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Archive {
            actor: actor.into(),
            reason: reason.into(),
        }
    }

    fn actor(&self) -> &str {
        match self {
            Self::Approve { actor } | Self::Reject { actor, .. } | Self::Archive { actor, .. } => {
                actor
            }
        }
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Self::Approve { .. } => None,
            Self::Reject { reason, .. } | Self::Archive { reason, .. } => Some(reason),
        }
    }

    fn action(&self) -> MemoryAuditAction {
        match self {
            Self::Approve { .. } => MemoryAuditAction::Approved,
            Self::Reject { .. } => MemoryAuditAction::Rejected,
            Self::Archive { .. } => MemoryAuditAction::Archived,
        }
    }

    fn next_status(&self) -> MemoryStatus {
        match self {
            Self::Approve { .. } => MemoryStatus::Approved,
            Self::Reject { .. } => MemoryStatus::Rejected,
            Self::Archive { .. } => MemoryStatus::Archived,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEdit {
    actor: String,
    content: String,
    reason: Option<String>,
}

impl MemoryEdit {
    pub fn replace_content(actor: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            content: content.into(),
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = non_empty_filter(reason.into());
        self
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    fn validate(&self) -> Result<(), MemoryError> {
        if self.content.trim().is_empty() {
            return Err(MemoryError::InvalidEdit {
                message: "memory content cannot be empty".to_string(),
            });
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAuditAction {
    CandidateCreated,
    Approved,
    Rejected,
    Archived,
    Edited,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAuditEvent {
    memory_id: String,
    sequence: u64,
    action: MemoryAuditAction,
    actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_content: Option<String>,
}

impl MemoryAuditEvent {
    fn candidate_created(memory_id: impl Into<String>) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: MemoryAuditAction::CandidateCreated,
            actor: "system".to_string(),
            reason: None,
            previous_content: None,
            new_content: None,
        }
    }

    fn reviewed(memory_id: impl Into<String>, decision: &ReviewDecision) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: decision.action(),
            actor: decision.actor().to_string(),
            reason: decision.reason().map(ToString::to_string),
            previous_content: None,
            new_content: None,
        }
    }

    fn edited(
        memory_id: impl Into<String>,
        edit: &MemoryEdit,
        previous_content: impl Into<String>,
        new_content: impl Into<String>,
    ) -> Self {
        Self {
            memory_id: memory_id.into(),
            sequence: 0,
            action: MemoryAuditAction::Edited,
            actor: edit.actor().to_string(),
            reason: edit.reason().map(ToString::to_string),
            previous_content: Some(previous_content.into()),
            new_content: Some(new_content.into()),
        }
    }

    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn action(&self) -> MemoryAuditAction {
        self.action
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn previous_content(&self) -> Option<&str> {
        self.previous_content.as_deref()
    }

    pub fn new_content(&self) -> Option<&str> {
        self.new_content.as_deref()
    }

    fn from_log_entry(entry: AppendLogEntry) -> Result<Self, MemoryError> {
        let mut event: Self = serde_json::from_value(entry.payload().clone())?;
        event.sequence = entry.sequence();
        Ok(event)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryQuery {
    project: ProjectRef,
    kind: Option<MemoryKind>,
    sensitivity: Option<MemorySensitivity>,
    tag: Option<String>,
    text: Option<String>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryListFilter {
    project: ProjectRef,
    status: Option<MemoryStatus>,
    kind: Option<MemoryKind>,
    sensitivity: Option<MemorySensitivity>,
    tag: Option<String>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySimilarityQuery {
    project: ProjectRef,
    kind: MemoryKind,
    content: String,
    min_score: u8,
    limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryStalenessQuery {
    project: ProjectRef,
    kind: Option<MemoryKind>,
    min_score: u8,
    limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryQualityQuery {
    project: ProjectRef,
    kind: Option<MemoryKind>,
    sensitivity: Option<MemorySensitivity>,
    tag: Option<String>,
    max_score: u8,
    limit: Option<usize>,
}

impl MemoryListFilter {
    pub fn new(project: ProjectRef) -> Self {
        Self {
            project,
            status: None,
            kind: None,
            sensitivity: None,
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
            kind: None,
            sensitivity: None,
            tag: None,
            max_score: 80,
            limit: None,
        }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrievedMemory {
    record: MemoryRecord,
    reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimilarMemory {
    record: MemoryRecord,
    score: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleMemory {
    record: MemoryRecord,
    newer_record: MemoryRecord,
    score: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityMemory {
    record: MemoryRecord,
    quality: MemoryQuality,
}

impl SimilarMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn score(&self) -> u8 {
        self.score
    }
}

impl StaleMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn newer_entry(&self) -> &MemoryEntry {
        self.newer_record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn newer_quality(&self) -> MemoryQuality {
        self.newer_record.quality()
    }

    pub fn score(&self) -> u8 {
        self.score
    }
}

impl RetrievedMemory {
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    pub fn quality(&self) -> MemoryQuality {
        self.record.quality()
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl QualityMemory {
    /// Returns the low-quality memory entry selected for review.
    pub fn entry(&self) -> &MemoryEntry {
        self.record.entry()
    }

    /// Returns the advisory score and findings that matched the query.
    pub fn quality(&self) -> &MemoryQuality {
        &self.quality
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("invalid memory candidate: {message}")]
    InvalidCandidate { message: String },

    #[error("invalid memory edit: {message}")]
    InvalidEdit { message: String },

    #[error("memory already exists: {id}")]
    MemoryAlreadyExists { id: String },

    #[error("duplicate memory: {id} duplicates {existing_id}")]
    DuplicateMemory { id: String, existing_id: String },

    #[error("memory not found: {id}")]
    MemoryNotFound { id: String },

    #[error("invalid memory transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: MemoryStatus,
        to: MemoryStatus,
    },

    #[error("storage failed: {0}")]
    Storage(#[from] StorageError),

    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub trait MemoryRepository {
    fn create_candidate(
        &mut self,
        input: MemoryCandidateInput,
    ) -> Result<MemoryRecord, MemoryError>;

    fn review(&mut self, id: &str, decision: ReviewDecision) -> Result<MemoryRecord, MemoryError>;

    fn edit(&mut self, id: &str, edit: MemoryEdit) -> Result<MemoryRecord, MemoryError>;

    fn get(&self, id: &str) -> Result<MemoryRecord, MemoryError>;

    fn audit_log(&self, id: &str) -> Result<Vec<MemoryAuditEvent>, MemoryError>;

    fn list(&self, filter: MemoryListFilter) -> Result<Vec<MemoryRecord>, MemoryError>;

    fn retrieve(&self, query: MemoryQuery) -> Result<Vec<RetrievedMemory>, MemoryError>;

    fn similar_active(
        &self,
        query: MemorySimilarityQuery,
    ) -> Result<Vec<SimilarMemory>, MemoryError>;

    fn stale_active(&self, query: MemoryStalenessQuery) -> Result<Vec<StaleMemory>, MemoryError>;

    /// Lists active memory whose advisory quality score is low enough for review.
    fn quality_review(&self, query: MemoryQualityQuery) -> Result<Vec<QualityMemory>, MemoryError>;
}

pub type InMemoryMemoryStore = StorageBackedMemoryStore<InMemoryStorage>;

pub struct StorageBackedMemoryStore<S> {
    scope: StorageScope,
    storage: S,
}

impl Default for StorageBackedMemoryStore<InMemoryStorage> {
    fn default() -> Self {
        Self::new(StorageScope::global(), InMemoryStorage::default())
    }
}

impl<S> StorageBackedMemoryStore<S> {
    pub fn new(scope: StorageScope, storage: S) -> Self {
        Self { scope, storage }
    }

    pub fn scope(&self) -> &StorageScope {
        &self.scope
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn into_inner(self) -> S {
        self.storage
    }
}

impl<S> MemoryRepository for StorageBackedMemoryStore<S>
where
    S: DocumentStore + AppendLogStore,
{
    fn create_candidate(
        &mut self,
        input: MemoryCandidateInput,
    ) -> Result<MemoryRecord, MemoryError> {
        input.validate()?;
        if self
            .storage
            .get_document(&self.scope, MEMORY_COLLECTION, &input.id)?
            .is_some()
        {
            return Err(MemoryError::MemoryAlreadyExists { id: input.id });
        }

        let entry = input.into_entry();
        if let Some(existing_id) = self.active_duplicate_id(&entry)? {
            return Err(MemoryError::DuplicateMemory {
                id: entry.id().to_string(),
                existing_id,
            });
        }

        let record = MemoryRecord::new(entry);
        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::candidate_created(record.entry().id()))?;
        self.append_index(MemoryAuditEvent::candidate_created(record.entry().id()))?;

        Ok(record)
    }

    fn review(&mut self, id: &str, decision: ReviewDecision) -> Result<MemoryRecord, MemoryError> {
        let current = self.get(id)?;
        ensure_transition(current.entry().status(), decision.next_status())?;
        let entry = set_status(current.entry().clone(), decision.next_status());
        let record = MemoryRecord::new(entry);

        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::reviewed(id, &decision))?;

        Ok(record)
    }

    fn edit(&mut self, id: &str, edit: MemoryEdit) -> Result<MemoryRecord, MemoryError> {
        edit.validate()?;
        let current = self.get(id)?;
        let previous_content = current.entry().content().to_string();
        let new_content = edit.content().to_string();
        let entry = replace_content(current.entry(), new_content.clone());
        let record = MemoryRecord::new(entry);

        self.put_record(&record)?;
        self.append_audit(MemoryAuditEvent::edited(
            id,
            &edit,
            previous_content,
            new_content,
        ))?;

        Ok(record)
    }

    fn get(&self, id: &str) -> Result<MemoryRecord, MemoryError> {
        self.storage
            .get_document(&self.scope, MEMORY_COLLECTION, id)?
            .ok_or_else(|| MemoryError::MemoryNotFound { id: id.to_string() })
            .and_then(|document| Ok(serde_json::from_value(document.payload().clone())?))
    }

    fn audit_log(&self, id: &str) -> Result<Vec<MemoryAuditEvent>, MemoryError> {
        self.storage
            .replay_log(&self.scope, &memory_audit_stream(id))?
            .into_iter()
            .map(MemoryAuditEvent::from_log_entry)
            .collect()
    }

    fn list(&self, filter: MemoryListFilter) -> Result<Vec<MemoryRecord>, MemoryError> {
        let mut records = Vec::new();
        for record in self.records()? {
            if record.entry().project().id() != filter.project.id() {
                continue;
            }

            if let Some(status) = filter.status
                && record.entry().status() != status
            {
                continue;
            }

            if let Some(kind) = filter.kind
                && record.entry().kind() != kind
            {
                continue;
            }

            if let Some(sensitivity) = filter.sensitivity
                && record.entry().sensitivity() != sensitivity
            {
                continue;
            }

            if let Some(tag) = &filter.tag
                && !record
                    .entry()
                    .tags()
                    .iter()
                    .any(|candidate| candidate == tag)
            {
                continue;
            }

            records.push(record);
        }

        records.sort_by(|left, right| left.entry().id().cmp(right.entry().id()));
        if let Some(limit) = filter.limit {
            records.truncate(limit);
        }
        Ok(records)
    }

    fn retrieve(&self, query: MemoryQuery) -> Result<Vec<RetrievedMemory>, MemoryError> {
        let mut retrieved = Vec::new();
        for record in self.records()? {
            if record.entry().status() != MemoryStatus::Approved {
                continue;
            }

            if record.entry().project().id() != query.project.id() {
                continue;
            }

            if let Some(sensitivity) = query.sensitivity {
                if record.entry().sensitivity() != sensitivity {
                    continue;
                }
            } else if record.entry().sensitivity() == MemorySensitivity::Sensitive {
                continue;
            }

            let mut reasons = Vec::new();
            if let Some(kind) = query.kind {
                if record.entry().kind() != kind {
                    continue;
                }
                reasons.push(format!("kind {}", memory_kind_label(kind)));
            }

            if let Some(tag) = &query.tag {
                if !record
                    .entry()
                    .tags()
                    .iter()
                    .any(|candidate| candidate == tag)
                {
                    continue;
                }
                reasons.push(format!("tag {tag}"));
            }

            if let Some(text) = &query.text {
                if !entry_content(record.entry())
                    .to_lowercase()
                    .contains(&text.to_lowercase())
                {
                    continue;
                }
                reasons.push(format!("text {text}"));
            }

            if let Some(sensitivity) = query.sensitivity {
                reasons.push(format!(
                    "sensitivity {}",
                    memory_sensitivity_label(sensitivity)
                ));
            }

            let reason = if reasons.is_empty() {
                "matched approved project memory".to_string()
            } else {
                format!("matched {}", reasons.join(" and "))
            };

            retrieved.push(RetrievedMemory { record, reason });
        }

        retrieved.sort_by(|left, right| left.entry().id().cmp(right.entry().id()));
        if let Some(limit) = query.limit {
            retrieved.truncate(limit);
        }
        Ok(retrieved)
    }

    fn similar_active(
        &self,
        query: MemorySimilarityQuery,
    ) -> Result<Vec<SimilarMemory>, MemoryError> {
        let query_tokens = content_tokens(&query.content);
        let mut similar = Vec::new();

        for record in self.records()? {
            let entry = record.entry();
            if !is_active_memory_status(entry.status()) {
                continue;
            }

            if entry.project().id() != query.project.id() || entry.kind() != query.kind {
                continue;
            }

            let score = token_similarity_score(&query_tokens, &content_tokens(entry.content()));
            if score < query.min_score {
                continue;
            }

            similar.push(SimilarMemory { record, score });
        }

        similar.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.entry().id().cmp(right.entry().id()))
        });
        if let Some(limit) = query.limit {
            similar.truncate(limit);
        }
        Ok(similar)
    }

    fn stale_active(&self, query: MemoryStalenessQuery) -> Result<Vec<StaleMemory>, MemoryError> {
        let records = self.indexed_records()?;
        let mut stale = Vec::new();

        for older in &records {
            let older_entry = older.record.entry();
            if older_entry.status() != MemoryStatus::Approved {
                continue;
            }

            if older_entry.project().id() != query.project.id() {
                continue;
            }

            if let Some(kind) = query.kind
                && older_entry.kind() != kind
            {
                continue;
            }

            let older_tokens = content_tokens(older_entry.content());
            let mut best_newer: Option<StaleMemory> = None;

            for newer in records
                .iter()
                .filter(|newer| newer.sequence > older.sequence)
            {
                let newer_entry = newer.record.entry();
                if !is_active_memory_status(newer_entry.status()) {
                    continue;
                }

                if newer_entry.project().id() != older_entry.project().id()
                    || newer_entry.kind() != older_entry.kind()
                {
                    continue;
                }

                let score =
                    token_similarity_score(&older_tokens, &content_tokens(newer_entry.content()));
                if score < query.min_score {
                    continue;
                }

                let candidate = StaleMemory {
                    record: older.record.clone(),
                    newer_record: newer.record.clone(),
                    score,
                };
                let replace_best = best_newer.as_ref().is_none_or(|best| {
                    candidate.score > best.score
                        || (candidate.score == best.score
                            && candidate.newer_entry().id() < best.newer_entry().id())
                });
                if replace_best {
                    best_newer = Some(candidate);
                }
            }

            if let Some(best_newer) = best_newer {
                stale.push(best_newer);
            }
        }

        stale.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.entry().id().cmp(right.entry().id()))
                .then_with(|| left.newer_entry().id().cmp(right.newer_entry().id()))
        });
        if let Some(limit) = query.limit {
            stale.truncate(limit);
        }
        Ok(stale)
    }

    fn quality_review(&self, query: MemoryQualityQuery) -> Result<Vec<QualityMemory>, MemoryError> {
        let mut low_quality = Vec::new();

        for record in self.records()? {
            let entry = record.entry();
            if !is_active_memory_status(entry.status()) {
                continue;
            }

            if entry.project().id() != query.project.id() {
                continue;
            }

            if query.kind.is_some_and(|kind| entry.kind() != kind) {
                continue;
            }

            if query
                .sensitivity
                .is_some_and(|sensitivity| entry.sensitivity() != sensitivity)
            {
                continue;
            }

            if query
                .tag
                .as_ref()
                .is_some_and(|tag| !entry.tags().iter().any(|entry_tag| entry_tag == tag))
            {
                continue;
            }

            let quality = record.quality();
            if quality.score() > query.max_score {
                continue;
            }

            low_quality.push(QualityMemory { record, quality });
        }

        low_quality.sort_by(|left, right| {
            left.quality
                .score()
                .cmp(&right.quality.score())
                .then_with(|| left.entry().id().cmp(right.entry().id()))
        });
        if let Some(limit) = query.limit {
            low_quality.truncate(limit);
        }
        Ok(low_quality)
    }
}

impl<S> StorageBackedMemoryStore<S>
where
    S: DocumentStore + AppendLogStore,
{
    fn put_record(&mut self, record: &MemoryRecord) -> Result<(), MemoryError> {
        self.storage.put_document(StorageDocument::new(
            self.scope.clone(),
            MEMORY_COLLECTION,
            record.entry().id(),
            serde_json::to_value(record)?,
        ))?;
        Ok(())
    }

    fn append_audit(&mut self, event: MemoryAuditEvent) -> Result<(), MemoryError> {
        self.storage.append_log(
            self.scope.clone(),
            memory_audit_stream(&event.memory_id),
            serde_json::to_value(event)?,
        )?;
        Ok(())
    }

    fn append_index(&mut self, event: MemoryAuditEvent) -> Result<(), MemoryError> {
        self.storage.append_log(
            self.scope.clone(),
            MEMORY_AUDIT_STREAM_PREFIX,
            serde_json::to_value(event)?,
        )?;
        Ok(())
    }

    fn records(&self) -> Result<Vec<MemoryRecord>, MemoryError> {
        Ok(self
            .indexed_records()?
            .into_iter()
            .map(|record| record.record)
            .collect())
    }

    fn indexed_records(&self) -> Result<Vec<IndexedMemoryRecord>, MemoryError> {
        // The first storage contract has point reads and append logs. Keep a
        // deterministic index as an append stream until queryable collections
        // are added to codel00p-storage.
        let mut records = Vec::new();
        for audit in self
            .storage
            .replay_log(&self.scope, MEMORY_AUDIT_STREAM_PREFIX)?
            .into_iter()
            .map(MemoryAuditEvent::from_log_entry)
        {
            let audit = audit?;
            if audit.action() == MemoryAuditAction::CandidateCreated {
                records.push(IndexedMemoryRecord {
                    sequence: audit.sequence(),
                    record: self.get(audit.memory_id())?,
                });
            }
        }
        Ok(records)
    }

    fn active_duplicate_id(&self, entry: &MemoryEntry) -> Result<Option<String>, MemoryError> {
        for record in self.records()? {
            let existing = record.entry();
            if is_active_memory_status(existing.status())
                && existing.project().id() == entry.project().id()
                && existing.kind() == entry.kind()
                && existing.content().trim() == entry.content().trim()
            {
                return Ok(Some(existing.id().to_string()));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndexedMemoryRecord {
    record: MemoryRecord,
    sequence: u64,
}

fn ensure_transition(from: MemoryStatus, to: MemoryStatus) -> Result<(), MemoryError> {
    let allowed = matches!(
        (from, to),
        (MemoryStatus::Candidate, MemoryStatus::Approved)
            | (MemoryStatus::Candidate, MemoryStatus::Rejected)
            | (MemoryStatus::Approved, MemoryStatus::Archived)
    );

    if allowed {
        Ok(())
    } else {
        Err(MemoryError::InvalidTransition { from, to })
    }
}

fn is_active_memory_status(status: MemoryStatus) -> bool {
    matches!(status, MemoryStatus::Candidate | MemoryStatus::Approved)
}

fn content_tokens(content: &str) -> BTreeSet<String> {
    content
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn token_similarity_score(left: &BTreeSet<String>, right: &BTreeSet<String>) -> u8 {
    if left.is_empty() || right.is_empty() {
        return 0;
    }

    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    (((intersection * 100) + (union / 2)) / union) as u8
}

fn score_memory_entry(entry: &MemoryEntry) -> MemoryQuality {
    let mut score = 100_i16;
    let mut findings = Vec::new();
    let tokens = content_tokens(entry.content());

    if tokens.len() < 8 {
        score -= 25;
        findings.push("content is too short to be reusable".to_string());
    }

    if entry.content().split_whitespace().count() > 80 {
        score -= 15;
        findings.push("content may be too long for frequent retrieval".to_string());
    }

    if contains_vague_language(&tokens) {
        score -= 10;
        findings.push("content uses vague language".to_string());
    }

    MemoryQuality {
        score: score.clamp(0, 100) as u8,
        findings,
    }
}

fn contains_vague_language(tokens: &BTreeSet<String>) -> bool {
    ["important", "stuff", "thing", "things", "this", "that"]
        .iter()
        .any(|token| tokens.contains(*token))
}

fn set_status(entry: MemoryEntry, status: MemoryStatus) -> MemoryEntry {
    entry.with_status(status)
}

fn replace_content(entry: &MemoryEntry, content: String) -> MemoryEntry {
    let mut updated = MemoryEntry::new(
        entry.id().to_string(),
        entry.project().clone(),
        entry.kind(),
        content,
    )
    .with_status(entry.status())
    .with_sensitivity(entry.sensitivity());
    if let Some(source) = entry.source() {
        updated = updated.with_source(source.clone());
    }
    for tag in entry.tags() {
        updated = updated.with_tag(tag.clone());
    }
    updated
}

fn entry_content(entry: &MemoryEntry) -> &str {
    entry.content()
}

fn non_empty_filter(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_remember_directive(line: &str) -> Option<(MemoryKind, Vec<String>, String)> {
    let line = line.trim();
    let rest = line.strip_prefix("remember")?.trim_start();
    let (header, content) = rest.split_once(':')?;
    let content = non_empty_filter(content.to_string())?;
    let header = header.trim();
    let kind = parse_directive_kind(header)?;
    let tags = parse_directive_tags(header);

    Some((kind, tags, content))
}

fn parse_directive_kind(header: &str) -> Option<MemoryKind> {
    if header.is_empty() {
        return Some(MemoryKind::Decision);
    }

    let kind = header
        .split_once('[')
        .map(|(kind, _)| kind)
        .unwrap_or(header)
        .trim();

    if kind.is_empty() {
        Some(MemoryKind::Decision)
    } else {
        memory_kind_from_label(kind)
    }
}

fn parse_directive_tags(header: &str) -> Vec<String> {
    let Some((_, raw_tags)) = header.split_once('[') else {
        return Vec::new();
    };
    let Some((raw_tags, _)) = raw_tags.split_once(']') else {
        return Vec::new();
    };

    raw_tags
        .split(',')
        .filter_map(|tag| non_empty_filter(tag.to_string()))
        .collect()
}

fn memory_kind_from_label(label: &str) -> Option<MemoryKind> {
    match label.trim().to_ascii_lowercase().as_str() {
        "architecture" => Some(MemoryKind::Architecture),
        "convention" => Some(MemoryKind::Convention),
        "workflow" => Some(MemoryKind::Workflow),
        "decision" => Some(MemoryKind::Decision),
        "deployment" => Some(MemoryKind::Deployment),
        "troubleshooting" => Some(MemoryKind::Troubleshooting),
        _ => None,
    }
}

fn memory_kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn memory_sensitivity_label(sensitivity: MemorySensitivity) -> &'static str {
    match sensitivity {
        MemorySensitivity::Normal => "normal",
        MemorySensitivity::Sensitive => "sensitive",
    }
}

fn memory_audit_stream(id: &str) -> String {
    format!("{MEMORY_AUDIT_STREAM_PREFIX}/{id}")
}

pub fn crate_name() -> &'static str {
    "codel00p-memory"
}
