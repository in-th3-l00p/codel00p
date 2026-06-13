use std::sync::{Arc, Mutex};

use codel00p_storage::{InMemoryStorage, StorageBackend};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::auth::JwtVerifier;
use crate::error::ApiError;

/// A live change notification broadcast to SSE subscribers in an organization.
#[derive(Clone, Debug, Serialize)]
pub struct ChangeEvent {
    pub org_id: String,
    pub entity: String,
    pub action: String,
}

impl ChangeEvent {
    fn new(org_id: impl Into<String>, entity: &str, action: &str) -> Self {
        Self {
            org_id: org_id.into(),
            entity: entity.to_string(),
            action: action.to_string(),
        }
    }
}

/// Shared service state: durable storage plus the token verifier. Cheap to clone
/// (everything behind `Arc`), as axum requires. The storage is a trait object so
/// any backend — in-memory, SQLite, Postgres — works without touching handlers.
#[derive(Clone)]
pub struct AppState {
    storage: Arc<Mutex<Box<dyn StorageBackend>>>,
    verifier: Arc<JwtVerifier>,
    events: broadcast::Sender<ChangeEvent>,
}

impl AppState {
    /// Builds state over a fresh in-memory store.
    pub fn new(verifier: JwtVerifier) -> Self {
        Self::with_storage(Box::new(InMemoryStorage::default()), verifier)
    }

    /// Builds state over an explicit storage backend (e.g. Postgres).
    pub fn with_storage(storage: Box<dyn StorageBackend>, verifier: JwtVerifier) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            storage: Arc::new(Mutex::new(storage)),
            verifier: Arc::new(verifier),
            events,
        }
    }

    pub fn verifier(&self) -> &JwtVerifier {
        &self.verifier
    }

    /// Broadcasts a change to live subscribers. A send with no subscribers is a
    /// no-op, so callers never need to check.
    pub(crate) fn publish(&self, org_id: &str, entity: &str, action: &str) {
        let _ = self.events.send(ChangeEvent::new(org_id, entity, action));
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<ChangeEvent> {
        self.events.subscribe()
    }

    /// Runs a storage operation on a blocking thread. The storage traits are
    /// synchronous (and a Postgres backend blocks), so they must not run on the
    /// async runtime's worker threads; `spawn_blocking` keeps the reactor free.
    pub async fn with_storage_blocking<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        F: FnOnce(&mut dyn StorageBackend) -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        let storage = self.storage.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = storage
                .lock()
                .map_err(|_| ApiError::Internal("storage lock poisoned".into()))?;
            f(&mut **guard)
        })
        .await
        .map_err(|error| ApiError::Internal(format!("storage task failed: {error}")))?
    }
}

/// Selects a storage backend from the environment. Returns `None` when no
/// `DATABASE_URL` is set, so the caller can fall back to in-memory storage.
/// Honouring `DATABASE_URL` requires the `postgres` feature.
pub fn storage_from_env()
-> Result<Option<Box<dyn StorageBackend>>, Box<dyn std::error::Error + Send + Sync>> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => return Ok(None),
    };

    #[cfg(feature = "postgres")]
    {
        let storage = codel00p_storage::PostgresStorage::connect(&url)?;
        Ok(Some(Box::new(storage)))
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = url;
        Err(
            "DATABASE_URL is set but codel00p-cloud was built without the `postgres` \
             feature; rebuild with `--features postgres`"
                .into(),
        )
    }
}
