use codel00p_protocol::{AgentEvent, EventId, SessionId, SessionMessage, SessionPersistenceEvent};
use codel00p_storage::{
    AppendLogEntry, AppendLogStore, DocumentStore, InMemoryStorage, StorageDocument, StorageError,
    StorageScope,
};
use serde::{Deserialize, Serialize};

const SESSION_METADATA_COLLECTION: &str = "sessions";
const SESSION_RECORD_STREAM_PREFIX: &str = "session";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetadata {
    session_id: SessionId,
    source: String,
    parent_session_id: Option<SessionId>,
}

impl SessionMetadata {
    pub fn new(session_id: SessionId, source: impl Into<String>) -> Self {
        Self {
            session_id,
            source: source.into(),
            parent_session_id: None,
        }
    }

    pub fn with_parent(mut self, parent_session_id: SessionId) -> Self {
        self.parent_session_id = Some(parent_session_id);
        self
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn parent_session_id(&self) -> Option<&SessionId> {
        self.parent_session_id.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SessionRecord {
    Message(SessionMessage),
    Event(AgentEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedSessionRecord {
    id: String,
    session_id: SessionId,
    sequence: u64,
    record: SessionRecord,
    persistence_event: SessionPersistenceEvent,
}

impl PersistedSessionRecord {
    fn from_log_entry(entry: AppendLogEntry) -> Result<Self, SessionStoreError> {
        let session_id = session_id_from_stream(entry.stream())?;
        let record = serde_json::from_value(entry.payload().clone())?;
        Ok(Self::with_id(
            session_id,
            entry.id().to_string(),
            entry.sequence(),
            record,
        ))
    }

    fn with_id(session_id: SessionId, id: String, sequence: u64, record: SessionRecord) -> Self {
        let persistence_event = SessionPersistenceEvent::record_appended(
            EventId::new(),
            session_id.clone(),
            id,
            sequence,
        );

        Self {
            id: persistence_event.record_id().to_string(),
            session_id,
            sequence,
            record,
            persistence_event,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn record(&self) -> &SessionRecord {
        &self.record
    }

    pub fn persistence_event(&self) -> &SessionPersistenceEvent {
        &self.persistence_event
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionStoreError {
    #[error("session already exists: {session_id}")]
    SessionAlreadyExists { session_id: String },

    #[error("session not found: {session_id}")]
    SessionNotFound { session_id: String },

    #[error("storage failed: {0}")]
    Storage(#[from] StorageError),

    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub trait SessionStore {
    fn create_session(&mut self, metadata: SessionMetadata) -> Result<(), SessionStoreError>;

    fn metadata(&self, session_id: &SessionId) -> Result<SessionMetadata, SessionStoreError>;

    fn append_message(
        &mut self,
        session_id: &SessionId,
        message: SessionMessage,
    ) -> Result<PersistedSessionRecord, SessionStoreError>;

    fn append_event(
        &mut self,
        session_id: &SessionId,
        event: AgentEvent,
    ) -> Result<PersistedSessionRecord, SessionStoreError>;

    fn replay(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<PersistedSessionRecord>, SessionStoreError>;
}

pub type InMemorySessionStore = StorageBackedSessionStore<InMemoryStorage>;

pub struct StorageBackedSessionStore<S> {
    scope: StorageScope,
    storage: S,
}

impl Default for StorageBackedSessionStore<InMemoryStorage> {
    fn default() -> Self {
        Self::new(StorageScope::global(), InMemoryStorage::default())
    }
}

impl<S> StorageBackedSessionStore<S> {
    pub fn new(scope: StorageScope, storage: S) -> Self {
        Self { scope, storage }
    }

    pub fn scope(&self) -> &StorageScope {
        &self.scope
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn into_inner(self) -> S {
        self.storage
    }
}

impl<S> StorageBackedSessionStore<S>
where
    S: DocumentStore + AppendLogStore,
{
    fn append_record(
        &mut self,
        session_id: &SessionId,
        record: SessionRecord,
    ) -> Result<PersistedSessionRecord, SessionStoreError> {
        self.ensure_session(session_id)?;
        let payload = serde_json::to_value(record)?;
        let entry = self.storage.append_log(
            self.scope.clone(),
            session_record_stream(session_id),
            payload,
        )?;

        PersistedSessionRecord::from_log_entry(entry)
    }

    fn ensure_session(&self, session_id: &SessionId) -> Result<(), SessionStoreError> {
        self.metadata(session_id).map(|_| ())
    }
}

impl<S> SessionStore for StorageBackedSessionStore<S>
where
    S: DocumentStore + AppendLogStore,
{
    fn create_session(&mut self, metadata: SessionMetadata) -> Result<(), SessionStoreError> {
        if self
            .storage
            .get_document(
                &self.scope,
                SESSION_METADATA_COLLECTION,
                metadata.session_id().as_str(),
            )?
            .is_some()
        {
            return Err(SessionStoreError::SessionAlreadyExists {
                session_id: metadata.session_id().as_str().to_string(),
            });
        }

        let session_id = metadata.session_id().as_str().to_string();
        let payload = serde_json::to_value(metadata)?;

        self.storage.put_document(StorageDocument::new(
            self.scope.clone(),
            SESSION_METADATA_COLLECTION,
            session_id,
            payload,
        ))?;

        Ok(())
    }

    fn metadata(&self, session_id: &SessionId) -> Result<SessionMetadata, SessionStoreError> {
        let document = self
            .storage
            .get_document(
                &self.scope,
                SESSION_METADATA_COLLECTION,
                session_id.as_str(),
            )?
            .ok_or_else(|| SessionStoreError::SessionNotFound {
                session_id: session_id.as_str().to_string(),
            })?;

        Ok(serde_json::from_value(document.payload().clone())?)
    }

    fn append_message(
        &mut self,
        session_id: &SessionId,
        message: SessionMessage,
    ) -> Result<PersistedSessionRecord, SessionStoreError> {
        self.append_record(session_id, SessionRecord::Message(message))
    }

    fn append_event(
        &mut self,
        session_id: &SessionId,
        event: AgentEvent,
    ) -> Result<PersistedSessionRecord, SessionStoreError> {
        self.append_record(session_id, SessionRecord::Event(event))
    }

    fn replay(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<PersistedSessionRecord>, SessionStoreError> {
        self.ensure_session(session_id)?;
        self.storage
            .replay_log(&self.scope, &session_record_stream(session_id))?
            .into_iter()
            .map(PersistedSessionRecord::from_log_entry)
            .collect()
    }
}

fn session_record_stream(session_id: &SessionId) -> String {
    format!("{SESSION_RECORD_STREAM_PREFIX}/{}", session_id.as_str())
}

fn session_id_from_stream(stream: &str) -> Result<SessionId, SessionStoreError> {
    let session_id = stream
        .strip_prefix(&format!("{SESSION_RECORD_STREAM_PREFIX}/"))
        .ok_or_else(|| {
            SessionStoreError::Storage(StorageError::Backend {
                message: format!("invalid session stream: {stream}"),
            })
        })?;

    Ok(serde_json::from_value(serde_json::Value::String(
        session_id.to_string(),
    ))?)
}

pub fn crate_name() -> &'static str {
    "codel00p-session"
}
