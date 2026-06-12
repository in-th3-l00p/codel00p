use std::{collections::BTreeMap, path::Path, time::Duration};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::Value;

use crate::{
    AppendLogEntry, AppendLogStore, DocumentStore, KeyValueStore, StorageDocument, StorageError,
    StorageScope, StorageValue,
};

pub struct SqliteStorage {
    connection: Connection,
}

impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path).map_err(sqlite_error)?;
        configure_connection(&connection)?;
        let storage = Self { connection };
        storage.initialize()?;
        Ok(storage)
    }

    pub fn in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory().map_err(sqlite_error)?;
        configure_connection(&connection)?;
        let storage = Self { connection };
        storage.initialize()?;
        Ok(storage)
    }

    fn initialize(&self) -> Result<(), StorageError> {
        self.connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS storage_values (
                    scope TEXT NOT NULL,
                    key TEXT NOT NULL,
                    version INTEGER NOT NULL,
                    payload TEXT NOT NULL,
                    metadata TEXT NOT NULL,
                    PRIMARY KEY (scope, key)
                );

                CREATE TABLE IF NOT EXISTS storage_documents (
                    scope TEXT NOT NULL,
                    collection TEXT NOT NULL,
                    id TEXT NOT NULL,
                    version INTEGER NOT NULL,
                    payload TEXT NOT NULL,
                    metadata TEXT NOT NULL,
                    PRIMARY KEY (scope, collection, id)
                );

                CREATE TABLE IF NOT EXISTS storage_logs (
                    scope TEXT NOT NULL,
                    stream TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    id TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    metadata TEXT NOT NULL,
                    PRIMARY KEY (scope, stream, sequence)
                );
                ",
            )
            .map_err(sqlite_error)
    }
}

impl KeyValueStore for SqliteStorage {
    fn put_value(&mut self, value: StorageValue) -> Result<StorageValue, StorageError> {
        let scope = scope_key(value.scope())?;
        let next_version = self
            .connection
            .query_row(
                "SELECT version FROM storage_values WHERE scope = ?1 AND key = ?2",
                params![scope, value.key()],
                |row| row.get::<_, u64>(0),
            )
            .optional()
            .map_err(sqlite_error)?
            .map(|version| version + 1)
            .unwrap_or(1);
        let stored = value.with_version(next_version);
        let payload = serde_json::to_string(stored.payload())?;
        let metadata = serde_json::to_string(stored.metadata())?;

        self.connection
            .execute(
                "
                INSERT INTO storage_values (scope, key, version, payload, metadata)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(scope, key) DO UPDATE SET
                    version = excluded.version,
                    payload = excluded.payload,
                    metadata = excluded.metadata
                ",
                params![scope, stored.key(), stored.version(), payload, metadata],
            )
            .map_err(sqlite_error)?;

        Ok(stored)
    }

    fn get_value(
        &self,
        scope: &StorageScope,
        key: &str,
    ) -> Result<Option<StorageValue>, StorageError> {
        let scope_key = scope_key(scope)?;
        let row = self
            .connection
            .query_row(
                "
                SELECT version, payload, metadata
                FROM storage_values
                WHERE scope = ?1 AND key = ?2
                ",
                params![scope_key, key],
                |row| {
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite_error)?;

        row.map(|(version, payload, metadata)| {
            Ok(StorageValue {
                scope: scope.clone(),
                key: key.to_string(),
                version,
                payload: serde_json::from_str(&payload)?,
                metadata: serde_json::from_str(&metadata)?,
            })
        })
        .transpose()
    }

    fn list_values(
        &self,
        scope: &StorageScope,
        key_prefix: Option<&str>,
    ) -> Result<Vec<StorageValue>, StorageError> {
        let scope_key = scope_key(scope)?;
        let mut values = Vec::new();

        match key_prefix {
            Some(prefix) => {
                let mut statement = self
                    .connection
                    .prepare(
                        "
                        SELECT key, version, payload, metadata
                        FROM storage_values
                        WHERE scope = ?1 AND key LIKE ?2
                        ORDER BY key ASC
                        ",
                    )
                    .map_err(sqlite_error)?;
                let rows = statement
                    .query_map(params![scope_key, format!("{prefix}%")], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })
                    .map_err(sqlite_error)?;
                for row in rows {
                    let (key, version, payload, metadata) = row.map_err(sqlite_error)?;
                    values.push(StorageValue {
                        scope: scope.clone(),
                        key,
                        version,
                        payload: serde_json::from_str(&payload)?,
                        metadata: serde_json::from_str(&metadata)?,
                    });
                }
            }
            None => {
                let mut statement = self
                    .connection
                    .prepare(
                        "
                        SELECT key, version, payload, metadata
                        FROM storage_values
                        WHERE scope = ?1
                        ORDER BY key ASC
                        ",
                    )
                    .map_err(sqlite_error)?;
                let rows = statement
                    .query_map(params![scope_key], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })
                    .map_err(sqlite_error)?;
                for row in rows {
                    let (key, version, payload, metadata) = row.map_err(sqlite_error)?;
                    values.push(StorageValue {
                        scope: scope.clone(),
                        key,
                        version,
                        payload: serde_json::from_str(&payload)?,
                        metadata: serde_json::from_str(&metadata)?,
                    });
                }
            }
        }

        Ok(values)
    }

    fn delete_value(&mut self, scope: &StorageScope, key: &str) -> Result<bool, StorageError> {
        let scope_key = scope_key(scope)?;
        let deleted = self
            .connection
            .execute(
                "
                DELETE FROM storage_values
                WHERE scope = ?1 AND key = ?2
                ",
                params![scope_key, key],
            )
            .map_err(sqlite_error)?;

        Ok(deleted > 0)
    }
}

impl DocumentStore for SqliteStorage {
    fn put_document(&mut self, document: StorageDocument) -> Result<StorageDocument, StorageError> {
        let scope = scope_key(document.scope())?;
        let next_version = self
            .connection
            .query_row(
                "
                SELECT version
                FROM storage_documents
                WHERE scope = ?1 AND collection = ?2 AND id = ?3
                ",
                params![scope, document.collection(), document.id()],
                |row| row.get::<_, u64>(0),
            )
            .optional()
            .map_err(sqlite_error)?
            .map(|version| version + 1)
            .unwrap_or(1);
        let stored = document.with_version(next_version);
        let payload = serde_json::to_string(stored.payload())?;
        let metadata = serde_json::to_string(stored.metadata())?;

        self.connection
            .execute(
                "
                INSERT INTO storage_documents (scope, collection, id, version, payload, metadata)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(scope, collection, id) DO UPDATE SET
                    version = excluded.version,
                    payload = excluded.payload,
                    metadata = excluded.metadata
                ",
                params![
                    scope,
                    stored.collection(),
                    stored.id(),
                    stored.version(),
                    payload,
                    metadata
                ],
            )
            .map_err(sqlite_error)?;

        Ok(stored)
    }

    fn get_document(
        &self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<Option<StorageDocument>, StorageError> {
        let scope_key = scope_key(scope)?;
        let row = self
            .connection
            .query_row(
                "
                SELECT version, payload, metadata
                FROM storage_documents
                WHERE scope = ?1 AND collection = ?2 AND id = ?3
                ",
                params![scope_key, collection, id],
                |row| {
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite_error)?;

        row.map(|(version, payload, metadata)| {
            Ok(StorageDocument {
                scope: scope.clone(),
                collection: collection.to_string(),
                id: id.to_string(),
                version,
                payload: serde_json::from_str(&payload)?,
                metadata: serde_json::from_str(&metadata)?,
            })
        })
        .transpose()
    }

    fn list_documents(
        &self,
        scope: &StorageScope,
        collection: &str,
    ) -> Result<Vec<StorageDocument>, StorageError> {
        let scope_key = scope_key(scope)?;
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT id, version, payload, metadata
                FROM storage_documents
                WHERE scope = ?1 AND collection = ?2
                ORDER BY id ASC
                ",
            )
            .map_err(sqlite_error)?;
        let rows = statement
            .query_map(params![scope_key, collection], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sqlite_error)?;

        let mut documents = Vec::new();
        for row in rows {
            let (id, version, payload, metadata) = row.map_err(sqlite_error)?;
            documents.push(StorageDocument {
                scope: scope.clone(),
                collection: collection.to_string(),
                id,
                version,
                payload: serde_json::from_str(&payload)?,
                metadata: serde_json::from_str(&metadata)?,
            });
        }

        Ok(documents)
    }

    fn delete_document(
        &mut self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<bool, StorageError> {
        let scope_key = scope_key(scope)?;
        let deleted = self
            .connection
            .execute(
                "DELETE FROM storage_documents WHERE scope = ?1 AND collection = ?2 AND id = ?3",
                params![scope_key, collection, id],
            )
            .map_err(sqlite_error)?;
        Ok(deleted > 0)
    }
}

impl AppendLogStore for SqliteStorage {
    fn append_log_entry(
        &mut self,
        scope: StorageScope,
        stream: String,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError> {
        let scope_key = scope_key(&scope)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sqlite_error)?;
        let sequence = transaction
            .query_row(
                "
                SELECT COALESCE(MAX(sequence), 0) + 1
                FROM storage_logs
                WHERE scope = ?1 AND stream = ?2
                ",
                params![scope_key, stream],
                |row| row.get::<_, u64>(0),
            )
            .map_err(sqlite_error)?;
        let entry = AppendLogEntry {
            scope,
            stream,
            id: format!("log-entry-{sequence}"),
            sequence,
            payload,
            metadata: BTreeMap::new(),
        };
        let payload = serde_json::to_string(entry.payload())?;
        let metadata = serde_json::to_string(entry.metadata())?;

        transaction
            .execute(
                "
                INSERT INTO storage_logs (scope, stream, sequence, id, payload, metadata)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    scope_key,
                    entry.stream(),
                    entry.sequence(),
                    entry.id(),
                    payload,
                    metadata
                ],
            )
            .map_err(sqlite_error)?;
        transaction.commit().map_err(sqlite_error)?;

        Ok(entry)
    }

    fn replay_log(
        &self,
        scope: &StorageScope,
        stream: &str,
    ) -> Result<Vec<AppendLogEntry>, StorageError> {
        let scope_key = scope_key(scope)?;
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT id, sequence, payload, metadata
                FROM storage_logs
                WHERE scope = ?1 AND stream = ?2
                ORDER BY sequence ASC
                ",
            )
            .map_err(sqlite_error)?;
        let rows = statement
            .query_map(params![scope_key, stream], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sqlite_error)?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, sequence, payload, metadata) = row.map_err(sqlite_error)?;
            entries.push(AppendLogEntry {
                scope: scope.clone(),
                stream: stream.to_string(),
                id,
                sequence,
                payload: serde_json::from_str(&payload)?,
                metadata: serde_json::from_str(&metadata)?,
            });
        }

        Ok(entries)
    }
}

fn scope_key(scope: &StorageScope) -> Result<String, StorageError> {
    Ok(serde_json::to_string(scope)?)
}

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sqlite_error)?;
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            ",
        )
        .map_err(sqlite_error)
}

fn sqlite_error(error: rusqlite::Error) -> StorageError {
    StorageError::Backend {
        message: error.to_string(),
    }
}
