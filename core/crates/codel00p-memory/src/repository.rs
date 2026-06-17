//! Repository trait and storage-backed repository handle.

use codel00p_protocol::MemoryEvidence;
use codel00p_storage::{InMemoryStorage, StorageScope};

use crate::{
    MemoryAuditEvent, MemoryCandidateInput, MemoryEdit, MemoryError, MemoryListFilter, MemoryMerge,
    MemoryQualityQuery, MemoryQuery, MemoryRecord, MemoryRetrievalQuery, MemoryRevision,
    MemorySimilarityQuery, MemorySplit, MemoryStalenessQuery, QualityMemory, RankedMemory,
    RetrievedMemory, ReviewDecision, SimilarMemory, StaleMemory,
};

pub trait MemoryRepository {
    fn create_candidate(
        &mut self,
        input: MemoryCandidateInput,
    ) -> Result<MemoryRecord, MemoryError>;

    fn review(&mut self, id: &str, decision: ReviewDecision) -> Result<MemoryRecord, MemoryError>;

    fn edit(&mut self, id: &str, edit: MemoryEdit) -> Result<MemoryRecord, MemoryError>;

    /// Appends an explicit evidence link to an existing active memory and records
    /// an [`crate::MemoryAuditAction::EvidenceAdded`] audit event. The optional
    /// `reason` and `actor` are carried onto that event. Returns the updated
    /// record.
    fn add_evidence(
        &mut self,
        id: &str,
        evidence: MemoryEvidence,
        actor: &str,
        reason: Option<String>,
    ) -> Result<MemoryRecord, MemoryError>;

    /// Folds the `source` memory into the `target`: archives the source, carries
    /// its tags onto the target, and records the merge on both audit logs.
    /// Returns the updated target record.
    fn merge(
        &mut self,
        source_id: &str,
        target_id: &str,
        merge: MemoryMerge,
    ) -> Result<MemoryRecord, MemoryError>;

    /// Splits the `source` memory into two: the source is kept active (optionally
    /// with updated content), and a new candidate memory is created carrying the
    /// split-off content plus the source's project/kind/sensitivity/tags and a
    /// source reference. Records a two-sided audit trail.
    /// Returns the newly created memory record.
    fn split(&mut self, source_id: &str, split: MemorySplit) -> Result<MemoryRecord, MemoryError>;

    fn get(&self, id: &str) -> Result<MemoryRecord, MemoryError>;

    fn audit_log(&self, id: &str) -> Result<Vec<MemoryAuditEvent>, MemoryError>;

    /// Reconstructs the ordered list of content snapshots from the audit trail.
    ///
    /// Returns one [`MemoryRevision`] per content-bearing audit event
    /// (`CandidateCreated` and `Edited`), in sequence order. Non-content events
    /// (approve, archive, merge, …) are skipped.
    fn revisions(&self, id: &str) -> Result<Vec<MemoryRevision>, MemoryError>;

    fn list(&self, filter: MemoryListFilter) -> Result<Vec<MemoryRecord>, MemoryError>;

    fn retrieve(&self, query: MemoryQuery) -> Result<Vec<RetrievedMemory>, MemoryError>;

    /// Ranks approved project memory by lexical similarity of the query string
    /// against each memory's content. Deterministic filters (kind/tag/sensitivity)
    /// are applied first to bound the candidate set, then results are ordered by
    /// descending score with ties broken by memory id. Reuses the same token
    /// similarity scorer as `similar_active`, so ranking is consistent across the
    /// product. Offline: no embeddings, no network.
    fn retrieve_ranked(
        &self,
        query: MemoryRetrievalQuery,
    ) -> Result<Vec<RankedMemory>, MemoryError>;

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
    pub(crate) scope: StorageScope,
    pub(crate) storage: S,
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
