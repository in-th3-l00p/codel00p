//! Error types returned by memory extraction and repository operations.

use codel00p_protocol::MemoryStatus;
use codel00p_storage::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("invalid memory candidate: {message}")]
    InvalidCandidate { message: String },

    #[error("invalid memory edit: {message}")]
    InvalidEdit { message: String },

    #[error("invalid memory merge: {message}")]
    InvalidMerge { message: String },

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
