use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use crate::{
    AppendLogEntry, AppendLogStore, DocumentStore, KeyValueStore, StorageDocument, StorageError,
    StorageScope, StorageValue,
};

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
