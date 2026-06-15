//! Repository trait and storage-backed repository handle.

use codel00p_storage::{InMemoryStorage, StorageScope};

use crate::{
    MemoryAuditEvent, MemoryCandidateInput, MemoryEdit, MemoryError, MemoryListFilter, MemoryMerge,
    MemoryQualityQuery, MemoryQuery, MemoryRecord, MemorySimilarityQuery, MemoryStalenessQuery,
    QualityMemory, RetrievedMemory, ReviewDecision, SimilarMemory, StaleMemory,
};

pub trait MemoryRepository {
    fn create_candidate(
        &mut self,
        input: MemoryCandidateInput,
    ) -> Result<MemoryRecord, MemoryError>;

    fn review(&mut self, id: &str, decision: ReviewDecision) -> Result<MemoryRecord, MemoryError>;

    fn edit(&mut self, id: &str, edit: MemoryEdit) -> Result<MemoryRecord, MemoryError>;

    /// Folds the `source` memory into the `target`: archives the source, carries
    /// its tags onto the target, and records the merge on both audit logs.
    /// Returns the updated target record.
    fn merge(
        &mut self,
        source_id: &str,
        target_id: &str,
        merge: MemoryMerge,
    ) -> Result<MemoryRecord, MemoryError>;

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
