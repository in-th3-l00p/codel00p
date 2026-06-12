use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStorage;

#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StorageScope {
    organization_id: Option<String>,
    project_id: Option<String>,
    workspace_id: Option<String>,
    user_id: Option<String>,
}

impl StorageScope {
    pub fn global() -> Self {
        Self::default()
    }

    pub fn organization(organization_id: impl Into<String>) -> Self {
        Self {
            organization_id: Some(organization_id.into()),
            project_id: None,
            workspace_id: None,
            user_id: None,
        }
    }

    pub fn project(organization_id: impl Into<String>, project_id: impl Into<String>) -> Self {
        Self {
            organization_id: Some(organization_id.into()),
            project_id: Some(project_id.into()),
            workspace_id: None,
            user_id: None,
        }
    }

    pub fn workspace(workspace_id: impl Into<String>) -> Self {
        Self {
            organization_id: None,
            project_id: None,
            workspace_id: Some(workspace_id.into()),
            user_id: None,
        }
    }

    pub fn user(user_id: impl Into<String>) -> Self {
        Self {
            organization_id: None,
            project_id: None,
            workspace_id: None,
            user_id: Some(user_id.into()),
        }
    }

    pub fn organization_id(&self) -> Option<&str> {
        self.organization_id.as_deref()
    }

    pub fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageDocument {
    scope: StorageScope,
    collection: String,
    id: String,
    version: u64,
    payload: Value,
    metadata: BTreeMap<String, String>,
}

impl StorageDocument {
    pub fn new(
        scope: StorageScope,
        collection: impl Into<String>,
        id: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            scope,
            collection: collection.into(),
            id: id.into(),
            version: 0,
            payload,
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn scope(&self) -> &StorageScope {
        &self.scope
    }

    pub fn collection(&self) -> &str {
        &self.collection
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageValue {
    scope: StorageScope,
    key: String,
    version: u64,
    payload: Value,
    metadata: BTreeMap<String, String>,
}

impl StorageValue {
    pub fn new(scope: StorageScope, key: impl Into<String>, payload: Value) -> Self {
        Self {
            scope,
            key: key.into(),
            version: 0,
            payload,
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn scope(&self) -> &StorageScope {
        &self.scope
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppendLogEntry {
    scope: StorageScope,
    stream: String,
    id: String,
    sequence: u64,
    payload: Value,
    metadata: BTreeMap<String, String>,
}

impl AppendLogEntry {
    pub fn scope(&self) -> &StorageScope {
        &self.scope
    }

    pub fn stream(&self) -> &str {
        &self.stream
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("record not found: {key}")]
    NotFound { key: String },

    #[error("storage conflict: {key}")]
    Conflict { key: String },

    #[error("serialization failed: {source}")]
    Serialization {
        #[from]
        source: serde_json::Error,
    },

    #[error("backend failure: {message}")]
    Backend { message: String },
}

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

#[derive(Default)]
pub struct InMemoryStorage {
    values: HashMap<ValueKey, StorageValue>,
    documents: HashMap<DocumentKey, StorageDocument>,
    logs: HashMap<LogKey, Vec<AppendLogEntry>>,
}

impl KeyValueStore for InMemoryStorage {
    fn put_value(&mut self, value: StorageValue) -> Result<StorageValue, StorageError> {
        let key = ValueKey::new(value.scope(), value.key());
        let next_version = self
            .values
            .get(&key)
            .map(|existing| existing.version() + 1)
            .unwrap_or(1);
        let stored = value.with_version(next_version);

        self.values.insert(key, stored.clone());

        Ok(stored)
    }

    fn get_value(
        &self,
        scope: &StorageScope,
        key: &str,
    ) -> Result<Option<StorageValue>, StorageError> {
        Ok(self.values.get(&ValueKey::new(scope, key)).cloned())
    }

    fn list_values(
        &self,
        scope: &StorageScope,
        key_prefix: Option<&str>,
    ) -> Result<Vec<StorageValue>, StorageError> {
        let mut values = self
            .values
            .iter()
            .filter(|(key, _)| &key.scope == scope)
            .filter(|(key, _)| {
                key_prefix
                    .map(|prefix| key.key.starts_with(prefix))
                    .unwrap_or(true)
            })
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();

        values.sort_by(|left, right| left.key().cmp(right.key()));

        Ok(values)
    }

    fn delete_value(&mut self, scope: &StorageScope, key: &str) -> Result<bool, StorageError> {
        Ok(self.values.remove(&ValueKey::new(scope, key)).is_some())
    }
}

impl DocumentStore for InMemoryStorage {
    fn put_document(&mut self, document: StorageDocument) -> Result<StorageDocument, StorageError> {
        let key = DocumentKey::new(document.scope(), document.collection(), document.id());
        let next_version = self
            .documents
            .get(&key)
            .map(|existing| existing.version() + 1)
            .unwrap_or(1);
        let stored = document.with_version(next_version);

        self.documents.insert(key, stored.clone());

        Ok(stored)
    }

    fn get_document(
        &self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<Option<StorageDocument>, StorageError> {
        Ok(self
            .documents
            .get(&DocumentKey::new(scope, collection, id))
            .cloned())
    }

    fn list_documents(
        &self,
        scope: &StorageScope,
        collection: &str,
    ) -> Result<Vec<StorageDocument>, StorageError> {
        let mut documents = self
            .documents
            .iter()
            .filter(|(key, _)| &key.scope == scope && key.collection == collection)
            .map(|(_, document)| document.clone())
            .collect::<Vec<_>>();

        documents.sort_by(|left, right| left.id().cmp(right.id()));

        Ok(documents)
    }

    fn delete_document(
        &mut self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<bool, StorageError> {
        Ok(self
            .documents
            .remove(&DocumentKey::new(scope, collection, id))
            .is_some())
    }
}

impl AppendLogStore for InMemoryStorage {
    fn append_log_entry(
        &mut self,
        scope: StorageScope,
        stream: String,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError> {
        let entries = self.logs.entry(LogKey::new(&scope, &stream)).or_default();
        let sequence = entries.len() as u64 + 1;
        let entry = AppendLogEntry {
            scope,
            stream,
            id: format!("log-entry-{sequence}"),
            sequence,
            payload,
            metadata: BTreeMap::new(),
        };

        entries.push(entry.clone());

        Ok(entry)
    }

    fn replay_log(
        &self,
        scope: &StorageScope,
        stream: &str,
    ) -> Result<Vec<AppendLogEntry>, StorageError> {
        Ok(self
            .logs
            .get(&LogKey::new(scope, stream))
            .cloned()
            .unwrap_or_default())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ValueKey {
    scope: StorageScope,
    key: String,
}

impl ValueKey {
    fn new(scope: &StorageScope, key: &str) -> Self {
        Self {
            scope: scope.clone(),
            key: key.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DocumentKey {
    scope: StorageScope,
    collection: String,
    id: String,
}

impl DocumentKey {
    fn new(scope: &StorageScope, collection: &str, id: &str) -> Self {
        Self {
            scope: scope.clone(),
            collection: collection.to_string(),
            id: id.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LogKey {
    scope: StorageScope,
    stream: String,
}

impl LogKey {
    fn new(scope: &StorageScope, stream: &str) -> Self {
        Self {
            scope: scope.clone(),
            stream: stream.to_string(),
        }
    }
}

pub fn crate_name() -> &'static str {
    "codel00p-storage"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn organization_scope_groups_documents_for_listing() {
        let mut storage = InMemoryStorage::default();
        let acme = StorageScope::organization("org_acme");
        let other = StorageScope::organization("org_other");

        assert_eq!(acme.organization_id(), Some("org_acme"));
        assert_eq!(acme.project_id(), None);

        storage
            .put_document(StorageDocument::new(
                acme.clone(),
                "projects",
                "proj_1",
                json!({ "name": "alpha" }),
            ))
            .expect("put alpha");
        storage
            .put_document(StorageDocument::new(
                acme.clone(),
                "projects",
                "proj_2",
                json!({ "name": "beta" }),
            ))
            .expect("put beta");
        storage
            .put_document(StorageDocument::new(
                other,
                "projects",
                "proj_3",
                json!({ "name": "gamma" }),
            ))
            .expect("put gamma");

        let listed = storage
            .list_documents(&acme, "projects")
            .expect("list org projects");

        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id(), "proj_1");
        assert_eq!(listed[1].id(), "proj_2");
    }

    #[test]
    fn documents_can_be_deleted() {
        let mut storage = InMemoryStorage::default();
        let scope = StorageScope::project("org_acme", "proj_1");
        storage
            .put_document(StorageDocument::new(
                scope.clone(),
                "agents",
                "agent_1",
                json!({ "name": "reviewer" }),
            ))
            .expect("put");

        assert!(
            storage
                .delete_document(&scope, "agents", "agent_1")
                .expect("delete")
        );
        assert!(
            !storage
                .delete_document(&scope, "agents", "agent_1")
                .expect("delete missing")
        );
        assert!(
            storage
                .get_document(&scope, "agents", "agent_1")
                .expect("get")
                .is_none()
        );
    }
}
