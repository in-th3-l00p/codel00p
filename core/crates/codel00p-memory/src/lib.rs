//! Reviewed project-memory extraction, lifecycle, retrieval, and quality review.
//!
//! This crate keeps extraction, review state, query builders, scoring, and storage
//! persistence in focused modules while preserving the original public facade.

mod error;
mod extraction;
mod inputs;
mod query;
mod records;
mod repository;
mod review;
mod store;
mod util;

pub use error::MemoryError;
pub use extraction::{ExplicitMemoryExtractor, MemoryCandidateExtractor};
pub use inputs::{MemoryCandidateInput, MemoryExtractionInput};
pub use query::{
    MemoryListFilter, MemoryQualityQuery, MemoryQuery, MemorySimilarityQuery, MemoryStalenessQuery,
};
pub use records::{
    MemoryQuality, MemoryRecord, QualityMemory, RetrievedMemory, SimilarMemory, StaleMemory,
};
pub use repository::{InMemoryMemoryStore, MemoryRepository, StorageBackedMemoryStore};
pub use review::{MemoryAuditAction, MemoryAuditEvent, MemoryEdit, ReviewDecision};

pub fn crate_name() -> &'static str {
    "codel00p-memory"
}
