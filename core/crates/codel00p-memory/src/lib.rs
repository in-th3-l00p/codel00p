//! Reviewed project-memory extraction, lifecycle, retrieval, and quality review.
//!
//! This crate keeps extraction, review state, query builders, scoring, and storage
//! persistence in focused modules while preserving the original public facade.

mod curator;
mod error;
mod extraction;
mod inputs;
mod query;
mod ranking;
mod records;
mod repository;
mod review;
mod store;
mod util;

pub use curator::{Consolidation, DEFAULT_CONSOLIDATION_THRESHOLD, DuplicateMemory, plan_consolidations};
pub use error::MemoryError;
pub use extraction::{ExplicitMemoryExtractor, MemoryCandidateExtractor};
pub use inputs::{MemoryCandidateInput, MemoryExtractionInput};
pub use query::{
    MemoryListFilter, MemoryQualityQuery, MemoryQuery, MemoryRetrievalQuery, MemorySimilarityQuery,
    MemoryStalenessQuery,
};
pub use ranking::{
    Bm25Ranker, Bm25RankingProvider, MemoryRanker, RankCandidate, RankedCandidate, RankingDocument,
    RankingProvider,
};
pub use records::{
    MemoryQuality, MemoryRecord, QualityMemory, RankedMemory, RetrievedMemory, SimilarMemory,
    StaleMemory,
};
pub use repository::{InMemoryMemoryStore, MemoryRepository, StorageBackedMemoryStore};
pub use review::{
    MemoryAuditAction, MemoryAuditEvent, MemoryEdit, MemoryMerge, MemoryRevision, MemorySplit,
    ReviewDecision,
};

pub fn crate_name() -> &'static str {
    "codel00p-memory"
}
