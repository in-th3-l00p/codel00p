//! Shared memory lifecycle test fixtures.

pub(crate) use codel00p_memory::{
    InMemoryMemoryStore, MemoryAuditAction, MemoryCandidateInput, MemoryEdit, MemoryError,
    MemoryListFilter, MemoryMerge, MemoryQualityQuery, MemoryQuery, MemoryRepository,
    MemorySimilarityQuery, MemoryStalenessQuery, ReviewDecision, StorageBackedMemoryStore,
};
pub(crate) use codel00p_protocol::{
    MemoryKind, MemorySensitivity, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId,
};
pub(crate) use codel00p_storage::{InMemoryStorage, StorageScope};

pub(crate) fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

pub(crate) fn source() -> MemorySource {
    MemorySource::turn(
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
    )
}
