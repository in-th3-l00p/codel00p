use std::collections::BTreeMap;

use codel00p_protocol::{AgentEvent, EventId, SessionId, SessionMessage, SessionPersistenceEvent};
use serde::{Deserialize, Serialize};

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
    fn new(session_id: SessionId, sequence: u64, record: SessionRecord) -> Self {
        let id = format!("record-{sequence}");
        let persistence_event = SessionPersistenceEvent::record_appended(
            EventId::new(),
            session_id.clone(),
            id.clone(),
            sequence,
        );

        Self {
            id,
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
}

pub trait SessionStore {
    fn create_session(&mut self, metadata: SessionMetadata) -> Result<(), SessionStoreError>;

    fn metadata(&self, session_id: &SessionId) -> Result<&SessionMetadata, SessionStoreError>;

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

#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: BTreeMap<SessionId, SessionMetadata>,
    records: BTreeMap<SessionId, Vec<PersistedSessionRecord>>,
}

impl InMemorySessionStore {
    fn append_record(
        &mut self,
        session_id: &SessionId,
        record: SessionRecord,
    ) -> Result<PersistedSessionRecord, SessionStoreError> {
        self.ensure_session(session_id)?;
        let records = self.records.entry(session_id.clone()).or_default();
        let sequence = records.len() as u64 + 1;
        let persisted = PersistedSessionRecord::new(session_id.clone(), sequence, record);
        records.push(persisted.clone());

        Ok(persisted)
    }

    fn ensure_session(&self, session_id: &SessionId) -> Result<(), SessionStoreError> {
        if self.sessions.contains_key(session_id) {
            return Ok(());
        }

        Err(SessionStoreError::SessionNotFound {
            session_id: session_id.as_str().to_string(),
        })
    }
}

impl SessionStore for InMemorySessionStore {
    fn create_session(&mut self, metadata: SessionMetadata) -> Result<(), SessionStoreError> {
        if self.sessions.contains_key(metadata.session_id()) {
            return Err(SessionStoreError::SessionAlreadyExists {
                session_id: metadata.session_id().as_str().to_string(),
            });
        }

        self.records
            .entry(metadata.session_id().clone())
            .or_default();
        self.sessions
            .insert(metadata.session_id().clone(), metadata);

        Ok(())
    }

    fn metadata(&self, session_id: &SessionId) -> Result<&SessionMetadata, SessionStoreError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| SessionStoreError::SessionNotFound {
                session_id: session_id.as_str().to_string(),
            })
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
        Ok(self.records.get(session_id).cloned().unwrap_or_default())
    }
}

pub fn crate_name() -> &'static str {
    "codel00p-session"
}
