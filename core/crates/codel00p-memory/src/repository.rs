//! Repository trait and storage-backed repository handle.

use std::sync::Arc;

use codel00p_protocol::MemoryEvidence;
use codel00p_storage::{InMemoryStorage, StorageScope};

use crate::{
    Bm25RankingProvider, MemoryAuditEvent, MemoryCandidateInput, MemoryEdit, MemoryError,
    MemoryListFilter, MemoryMerge, MemoryQualityQuery, MemoryQuery, MemoryRecord,
    MemoryRetrievalQuery, MemoryRevision, MemorySimilarityQuery, MemorySplit, MemoryStalenessQuery,
    QualityMemory, RankedMemory, RankingProvider, RetrievedMemory, ReviewDecision, SimilarMemory,
    StaleMemory,
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

    /// Ranks approved project memory against the query string with BM25.
    /// Deterministic filters (kind/tag/sensitivity/visibility) are applied first
    /// to bound the candidate set, which then forms the corpus BM25 scores over
    /// (term-frequency saturation + inverse-document-frequency). Raw BM25 scores
    /// are mapped onto the 0..=100 scale; results are ordered by descending score
    /// with ties broken by memory id. Offline: no embeddings, no network. See
    /// [`crate::Bm25Ranker`] / [`crate::MemoryRanker`] for the pluggable seam.
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
    /// The ranking provider consulted by `retrieve_ranked`. Defaults to offline
    /// BM25 ([`Bm25RankingProvider`]); the host injects an external provider via
    /// [`with_ranker`](Self::with_ranker) when the operator has opted in.
    pub(crate) ranker: Arc<dyn RankingProvider>,
}

impl Default for StorageBackedMemoryStore<InMemoryStorage> {
    fn default() -> Self {
        Self::new(StorageScope::global(), InMemoryStorage::default())
    }
}

impl<S> StorageBackedMemoryStore<S> {
    pub fn new(scope: StorageScope, storage: S) -> Self {
        Self {
            scope,
            storage,
            ranker: Arc::new(Bm25RankingProvider),
        }
    }

    /// Inject a custom ranking provider for relevance retrieval (`retrieve_ranked`).
    /// All other operations are unaffected. Use this to wire an external ranker;
    /// without it the store ranks with offline BM25.
    pub fn with_ranker(mut self, ranker: Arc<dyn RankingProvider>) -> Self {
        self.ranker = ranker;
        self
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
