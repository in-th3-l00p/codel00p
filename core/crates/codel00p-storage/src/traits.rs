use serde_json::Value;

use crate::{AppendLogEntry, StorageDocument, StorageError, StorageScope, StorageValue};

pub trait DocumentStore {
    fn put_document(&mut self, document: StorageDocument) -> Result<StorageDocument, StorageError>;

    fn get_document(
        &self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<Option<StorageDocument>, StorageError>;

    fn list_documents(
        &self,
        scope: &StorageScope,
        collection: &str,
    ) -> Result<Vec<StorageDocument>, StorageError>;

    /// Removes a document, returning whether one existed.
    fn delete_document(
        &mut self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<bool, StorageError>;
}

pub trait KeyValueStore {
    fn put_value(&mut self, value: StorageValue) -> Result<StorageValue, StorageError>;

    fn get_value(
        &self,
        scope: &StorageScope,
        key: &str,
    ) -> Result<Option<StorageValue>, StorageError>;

    fn list_values(
        &self,
        scope: &StorageScope,
        key_prefix: Option<&str>,
    ) -> Result<Vec<StorageValue>, StorageError>;

    fn delete_value(&mut self, scope: &StorageScope, key: &str) -> Result<bool, StorageError>;
}

pub trait AppendLogStore {
    /// Object-safe append. Implementors provide this; most callers prefer the
    /// `append_log` convenience below.
    fn append_log_entry(
        &mut self,
        scope: StorageScope,
        stream: String,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError>;

    /// Ergonomic append accepting anything string-like. Marked `Self: Sized` so
    /// it stays out of the vtable, keeping the trait object-safe (a `dyn`
    /// backend appends via [`AppendLogStore::append_log_entry`]).
    fn append_log(
        &mut self,
        scope: StorageScope,
        stream: impl Into<String>,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError>
    where
        Self: Sized,
    {
        self.append_log_entry(scope, stream.into(), payload)
    }

    fn replay_log(
        &self,
        scope: &StorageScope,
        stream: &str,
    ) -> Result<Vec<AppendLogEntry>, StorageError>;
}

/// A complete storage backend: documents, key-value state, and append logs. The
/// blanket implementation means every type that provides all three primitives is
/// usable wherever a full backend is required (e.g. behind a trait object in a
/// service), so backends can be swapped — in-memory, SQLite, Postgres — without
/// changing call sites.
pub trait StorageBackend: DocumentStore + KeyValueStore + AppendLogStore + Send {}

impl<T> StorageBackend for T where T: DocumentStore + KeyValueStore + AppendLogStore + Send {}
