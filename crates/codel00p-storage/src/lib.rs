use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}

pub trait AppendLogStore {
    fn append_log(
        &mut self,
        scope: StorageScope,
        stream: impl Into<String>,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError>;

    fn replay_log(
        &self,
        scope: &StorageScope,
        stream: &str,
    ) -> Result<Vec<AppendLogEntry>, StorageError>;
}

#[derive(Default)]
pub struct InMemoryStorage {
    documents: HashMap<DocumentKey, StorageDocument>,
    logs: HashMap<LogKey, Vec<AppendLogEntry>>,
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
}

impl AppendLogStore for InMemoryStorage {
    fn append_log(
        &mut self,
        scope: StorageScope,
        stream: impl Into<String>,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError> {
        let stream = stream.into();
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
