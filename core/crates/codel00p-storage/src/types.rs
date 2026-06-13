use std::collections::BTreeMap;

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
    pub(crate) scope: StorageScope,
    pub(crate) collection: String,
    pub(crate) id: String,
    pub(crate) version: u64,
    pub(crate) payload: Value,
    pub(crate) metadata: BTreeMap<String, String>,
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

    pub(crate) fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageValue {
    pub(crate) scope: StorageScope,
    pub(crate) key: String,
    pub(crate) version: u64,
    pub(crate) payload: Value,
    pub(crate) metadata: BTreeMap<String, String>,
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

    pub(crate) fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppendLogEntry {
    pub(crate) scope: StorageScope,
    pub(crate) stream: String,
    pub(crate) id: String,
    pub(crate) sequence: u64,
    pub(crate) payload: Value,
    pub(crate) metadata: BTreeMap<String, String>,
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
