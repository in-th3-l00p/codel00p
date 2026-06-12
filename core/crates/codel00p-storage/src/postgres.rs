use std::collections::BTreeMap;
use std::sync::Mutex;

use ::postgres::Client;
use postgres_native_tls::MakeTlsConnector;
use serde_json::Value;

use crate::{
    AppendLogEntry, AppendLogStore, DocumentStore, KeyValueStore, StorageDocument, StorageError,
    StorageScope, StorageValue,
};

/// A Postgres-backed storage backend, the durable home for cloud product data.
///
/// Payloads and metadata are stored as `jsonb`. The client is held behind a
/// `Mutex` so the read methods (which take `&self`) can borrow it mutably, as
/// the blocking `postgres` client requires. The schema mirrors the SQLite
/// backend so the two are interchangeable behind the storage traits.
pub struct PostgresStorage {
    client: Mutex<Client>,
}

impl PostgresStorage {
    /// Connects to Postgres from a libpq-style connection string. A TLS
    /// connector is always supplied; servers without SSL (e.g. local Docker)
    /// transparently fall back to a plaintext connection, while managed hosts
    /// such as Neon and Supabase negotiate TLS.
    pub fn connect(url: &str) -> Result<Self, StorageError> {
        let connector = native_tls::TlsConnector::new().map_err(|error| StorageError::Backend {
            message: format!("tls connector: {error}"),
        })?;
        let client = Client::connect(url, MakeTlsConnector::new(connector)).map_err(pg_error)?;
        let storage = Self {
            client: Mutex::new(client),
        };
        storage.initialize()?;
        Ok(storage)
    }

    fn initialize(&self) -> Result<(), StorageError> {
        self.with_client(|client| {
            // A batch in the simple query protocol runs as one implicit
            // transaction, so the transaction-level advisory lock serializes
            // concurrent initializers — `CREATE TABLE IF NOT EXISTS` is not
            // race-safe under concurrent DDL in Postgres.
            client
                .batch_execute(
                    "
                    SELECT pg_advisory_xact_lock(4920263);

                    CREATE TABLE IF NOT EXISTS storage_values (
                        scope TEXT NOT NULL,
                        key TEXT NOT NULL,
                        version BIGINT NOT NULL,
                        payload JSONB NOT NULL,
                        metadata JSONB NOT NULL,
                        PRIMARY KEY (scope, key)
                    );

                    CREATE TABLE IF NOT EXISTS storage_documents (
                        scope TEXT NOT NULL,
                        collection TEXT NOT NULL,
                        id TEXT NOT NULL,
                        version BIGINT NOT NULL,
                        payload JSONB NOT NULL,
                        metadata JSONB NOT NULL,
                        PRIMARY KEY (scope, collection, id)
                    );

                    CREATE TABLE IF NOT EXISTS storage_logs (
                        scope TEXT NOT NULL,
                        stream TEXT NOT NULL,
                        sequence BIGINT NOT NULL,
                        id TEXT NOT NULL,
                        payload JSONB NOT NULL,
                        metadata JSONB NOT NULL,
                        PRIMARY KEY (scope, stream, sequence)
                    );
                    ",
                )
                .map_err(pg_error)
        })
    }

    fn with_client<T>(
        &self,
        f: impl FnOnce(&mut Client) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::Backend {
            message: "postgres client mutex poisoned".to_string(),
        })?;
        f(&mut client)
    }
}

impl KeyValueStore for PostgresStorage {
    fn put_value(&mut self, value: StorageValue) -> Result<StorageValue, StorageError> {
        let scope = scope_key(value.scope())?;
        self.with_client(|client| {
            let next_version = client
                .query_opt(
                    "SELECT version FROM storage_values WHERE scope = $1 AND key = $2",
                    &[&scope, &value.key()],
                )
                .map_err(pg_error)?
                .map(|row| row.get::<_, i64>(0) as u64 + 1)
                .unwrap_or(1);
            let stored = value.with_version(next_version);
            let metadata = serde_json::to_value(stored.metadata())?;

            client
                .execute(
                    "
                    INSERT INTO storage_values (scope, key, version, payload, metadata)
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT (scope, key) DO UPDATE SET
                        version = EXCLUDED.version,
                        payload = EXCLUDED.payload,
                        metadata = EXCLUDED.metadata
                    ",
                    &[
                        &scope,
                        &stored.key(),
                        &(stored.version() as i64),
                        stored.payload(),
                        &metadata,
                    ],
                )
                .map_err(pg_error)?;

            Ok(stored)
        })
    }

    fn get_value(
        &self,
        scope: &StorageScope,
        key: &str,
    ) -> Result<Option<StorageValue>, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let row = client
                .query_opt(
                    "SELECT version, payload, metadata FROM storage_values WHERE scope = $1 AND key = $2",
                    &[&scope_text, &key],
                )
                .map_err(pg_error)?;

            row.map(|row| {
                Ok(StorageValue {
                    scope: scope.clone(),
                    key: key.to_string(),
                    version: row.get::<_, i64>(0) as u64,
                    payload: row.get::<_, Value>(1),
                    metadata: metadata_from_value(row.get::<_, Value>(2))?,
                })
            })
            .transpose()
        })
    }

    fn list_values(
        &self,
        scope: &StorageScope,
        key_prefix: Option<&str>,
    ) -> Result<Vec<StorageValue>, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let rows = match key_prefix {
                Some(prefix) => client
                    .query(
                        "
                        SELECT key, version, payload, metadata
                        FROM storage_values
                        WHERE scope = $1 AND key LIKE $2
                        ORDER BY key ASC
                        ",
                        &[&scope_text, &format!("{prefix}%")],
                    )
                    .map_err(pg_error)?,
                None => client
                    .query(
                        "
                        SELECT key, version, payload, metadata
                        FROM storage_values
                        WHERE scope = $1
                        ORDER BY key ASC
                        ",
                        &[&scope_text],
                    )
                    .map_err(pg_error)?,
            };

            rows.into_iter()
                .map(|row| {
                    Ok(StorageValue {
                        scope: scope.clone(),
                        key: row.get::<_, String>(0),
                        version: row.get::<_, i64>(1) as u64,
                        payload: row.get::<_, Value>(2),
                        metadata: metadata_from_value(row.get::<_, Value>(3))?,
                    })
                })
                .collect()
        })
    }

    fn delete_value(&mut self, scope: &StorageScope, key: &str) -> Result<bool, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let deleted = client
                .execute(
                    "DELETE FROM storage_values WHERE scope = $1 AND key = $2",
                    &[&scope_text, &key],
                )
                .map_err(pg_error)?;
            Ok(deleted > 0)
        })
    }
}

impl DocumentStore for PostgresStorage {
    fn put_document(&mut self, document: StorageDocument) -> Result<StorageDocument, StorageError> {
        let scope = scope_key(document.scope())?;
        self.with_client(|client| {
            let next_version = client
                .query_opt(
                    "
                    SELECT version FROM storage_documents
                    WHERE scope = $1 AND collection = $2 AND id = $3
                    ",
                    &[&scope, &document.collection(), &document.id()],
                )
                .map_err(pg_error)?
                .map(|row| row.get::<_, i64>(0) as u64 + 1)
                .unwrap_or(1);
            let stored = document.with_version(next_version);
            let metadata = serde_json::to_value(stored.metadata())?;

            client
                .execute(
                    "
                    INSERT INTO storage_documents (scope, collection, id, version, payload, metadata)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (scope, collection, id) DO UPDATE SET
                        version = EXCLUDED.version,
                        payload = EXCLUDED.payload,
                        metadata = EXCLUDED.metadata
                    ",
                    &[
                        &scope,
                        &stored.collection(),
                        &stored.id(),
                        &(stored.version() as i64),
                        stored.payload(),
                        &metadata,
                    ],
                )
                .map_err(pg_error)?;

            Ok(stored)
        })
    }

    fn get_document(
        &self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<Option<StorageDocument>, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let row = client
                .query_opt(
                    "
                    SELECT version, payload, metadata FROM storage_documents
                    WHERE scope = $1 AND collection = $2 AND id = $3
                    ",
                    &[&scope_text, &collection, &id],
                )
                .map_err(pg_error)?;

            row.map(|row| {
                Ok(StorageDocument {
                    scope: scope.clone(),
                    collection: collection.to_string(),
                    id: id.to_string(),
                    version: row.get::<_, i64>(0) as u64,
                    payload: row.get::<_, Value>(1),
                    metadata: metadata_from_value(row.get::<_, Value>(2))?,
                })
            })
            .transpose()
        })
    }

    fn list_documents(
        &self,
        scope: &StorageScope,
        collection: &str,
    ) -> Result<Vec<StorageDocument>, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let rows = client
                .query(
                    "
                    SELECT id, version, payload, metadata FROM storage_documents
                    WHERE scope = $1 AND collection = $2
                    ORDER BY id ASC
                    ",
                    &[&scope_text, &collection],
                )
                .map_err(pg_error)?;

            rows.into_iter()
                .map(|row| {
                    Ok(StorageDocument {
                        scope: scope.clone(),
                        collection: collection.to_string(),
                        id: row.get::<_, String>(0),
                        version: row.get::<_, i64>(1) as u64,
                        payload: row.get::<_, Value>(2),
                        metadata: metadata_from_value(row.get::<_, Value>(3))?,
                    })
                })
                .collect()
        })
    }

    fn delete_document(
        &mut self,
        scope: &StorageScope,
        collection: &str,
        id: &str,
    ) -> Result<bool, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let deleted = client
                .execute(
                    "DELETE FROM storage_documents WHERE scope = $1 AND collection = $2 AND id = $3",
                    &[&scope_text, &collection, &id],
                )
                .map_err(pg_error)?;
            Ok(deleted > 0)
        })
    }
}

impl AppendLogStore for PostgresStorage {
    fn append_log_entry(
        &mut self,
        scope: StorageScope,
        stream: String,
        payload: Value,
    ) -> Result<AppendLogEntry, StorageError> {
        let scope_text = scope_key(&scope)?;
        self.with_client(|client| {
            let mut transaction = client.transaction().map_err(pg_error)?;
            let sequence = transaction
                .query_one(
                    "
                    SELECT COALESCE(MAX(sequence), 0) + 1
                    FROM storage_logs
                    WHERE scope = $1 AND stream = $2
                    ",
                    &[&scope_text, &stream],
                )
                .map_err(pg_error)?
                .get::<_, i64>(0) as u64;
            let entry = AppendLogEntry {
                scope: scope.clone(),
                stream: stream.clone(),
                id: format!("log-entry-{sequence}"),
                sequence,
                payload,
                metadata: BTreeMap::new(),
            };
            let metadata = serde_json::to_value(entry.metadata())?;

            transaction
                .execute(
                    "
                    INSERT INTO storage_logs (scope, stream, sequence, id, payload, metadata)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ",
                    &[
                        &scope_text,
                        &entry.stream(),
                        &(entry.sequence() as i64),
                        &entry.id(),
                        entry.payload(),
                        &metadata,
                    ],
                )
                .map_err(pg_error)?;
            transaction.commit().map_err(pg_error)?;

            Ok(entry)
        })
    }

    fn replay_log(
        &self,
        scope: &StorageScope,
        stream: &str,
    ) -> Result<Vec<AppendLogEntry>, StorageError> {
        let scope_text = scope_key(scope)?;
        self.with_client(|client| {
            let rows = client
                .query(
                    "
                    SELECT id, sequence, payload, metadata FROM storage_logs
                    WHERE scope = $1 AND stream = $2
                    ORDER BY sequence ASC
                    ",
                    &[&scope_text, &stream],
                )
                .map_err(pg_error)?;

            rows.into_iter()
                .map(|row| {
                    Ok(AppendLogEntry {
                        scope: scope.clone(),
                        stream: stream.to_string(),
                        id: row.get::<_, String>(0),
                        sequence: row.get::<_, i64>(1) as u64,
                        payload: row.get::<_, Value>(2),
                        metadata: metadata_from_value(row.get::<_, Value>(3))?,
                    })
                })
                .collect()
        })
    }
}

fn scope_key(scope: &StorageScope) -> Result<String, StorageError> {
    Ok(serde_json::to_string(scope)?)
}

fn metadata_from_value(value: Value) -> Result<BTreeMap<String, String>, StorageError> {
    Ok(serde_json::from_value(value)?)
}

fn pg_error(error: ::postgres::Error) -> StorageError {
    use std::error::Error;

    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    StorageError::Backend { message }
}
