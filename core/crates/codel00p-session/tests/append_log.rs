use codel00p_protocol::{
    AgentEvent, EventId, SessionId, SessionMessage, SessionPersistenceEvent, TurnId,
};
use codel00p_session::{
    InMemorySessionStore, SessionMetadata, SessionRecord, SessionStore, SessionStoreError,
    StorageBackedSessionStore,
};
use codel00p_storage::{InMemoryStorage, StorageScope};

#[test]
fn appends_and_replays_session_records_in_order() {
    let mut store = InMemorySessionStore::default();
    let session_id = SessionId::from_static("session-log");
    store
        .create_session(SessionMetadata::new(session_id.clone(), "cli"))
        .expect("create session");

    let first = store
        .append_message(&session_id, SessionMessage::user("Inspect the project."))
        .expect("append message");
    let second = store
        .append_event(
            &session_id,
            AgentEvent::TurnStarted {
                event_id: EventId::from_static("event-turn"),
                session_id: session_id.clone(),
                turn_id: TurnId::from_static("turn-1"),
            },
        )
        .expect("append event");

    let records = store.replay(&session_id).expect("replay");

    assert_eq!(first.sequence(), 1);
    assert_eq!(second.sequence(), 2);
    assert_eq!(records.len(), 2);
    assert!(matches!(records[0].record(), SessionRecord::Message(_)));
    assert!(matches!(records[1].record(), SessionRecord::Event(_)));
}

#[test]
fn records_parent_session_lineage() {
    let mut store = InMemorySessionStore::default();
    let parent = SessionId::from_static("session-parent");
    let child = SessionId::from_static("session-child");

    store
        .create_session(SessionMetadata::new(parent.clone(), "cli"))
        .expect("create parent");
    store
        .create_session(SessionMetadata::new(child.clone(), "cli").with_parent(parent.clone()))
        .expect("create child");

    assert_eq!(
        store
            .metadata(&child)
            .expect("child metadata")
            .parent_session_id(),
        Some(&parent)
    );
}

#[test]
fn rejects_records_for_unknown_session() {
    let mut store = InMemorySessionStore::default();
    let error = store
        .append_message(
            &SessionId::from_static("session-missing"),
            SessionMessage::assistant("No session."),
        )
        .expect_err("unknown session should fail");

    assert!(matches!(error, SessionStoreError::SessionNotFound { .. }));
}

#[test]
fn persistence_events_are_emitted_for_appends() {
    let mut store = InMemorySessionStore::default();
    let session_id = SessionId::from_static("session-persistence-event");
    store
        .create_session(SessionMetadata::new(session_id.clone(), "cli"))
        .expect("create session");

    let appended = store
        .append_message(&session_id, SessionMessage::user("Persist this."))
        .expect("append message");

    assert_eq!(appended.persistence_event().sequence(), 1);
    assert_eq!(appended.persistence_event().record_id(), appended.id());
    assert!(matches!(
        appended.persistence_event(),
        SessionPersistenceEvent::RecordAppended { .. }
    ));
}

#[test]
fn session_store_can_run_on_supplied_storage_backend_and_scope() {
    let storage = InMemoryStorage::default();
    let scope = StorageScope::project("org-1", "project-1");
    let mut store = StorageBackedSessionStore::new(scope, storage);
    let session_id = SessionId::from_static("session-storage-backed");

    store
        .create_session(SessionMetadata::new(session_id.clone(), "cli"))
        .expect("create session");
    store
        .append_message(&session_id, SessionMessage::user("Use generic storage."))
        .expect("append message");

    let replayed = store.replay(&session_id).expect("replay");

    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].session_id(), &session_id);
    assert!(matches!(replayed[0].record(), SessionRecord::Message(_)));
}

#[test]
fn lists_all_sessions_in_scope() {
    let mut store = StorageBackedSessionStore::new(
        StorageScope::project("org-1", "project-1"),
        InMemoryStorage::default(),
    );

    store
        .create_session(SessionMetadata::new(
            SessionId::from_static("session-b"),
            "cli",
        ))
        .expect("create session b");
    store
        .create_session(SessionMetadata::new(
            SessionId::from_static("session-a"),
            "chat",
        ))
        .expect("create session a");

    let sessions = store.list_sessions().expect("list sessions");
    let ids: Vec<&str> = sessions
        .iter()
        .map(|metadata| metadata.session_id().as_str())
        .collect();

    assert_eq!(ids, ["session-a", "session-b"]);
    assert_eq!(sessions[1].source(), "cli");
}

#[test]
fn lists_no_sessions_when_scope_is_empty() {
    let store = StorageBackedSessionStore::new(
        StorageScope::project("org-1", "empty"),
        InMemoryStorage::default(),
    );

    assert!(store.list_sessions().expect("list sessions").is_empty());
}

#[test]
fn created_at_round_trips_through_the_store() {
    let mut store = StorageBackedSessionStore::new(
        StorageScope::project("org-1", "project-1"),
        InMemoryStorage::default(),
    );
    let dated = SessionId::from_static("session-dated");
    let undated = SessionId::from_static("session-undated");

    store
        .create_session(
            SessionMetadata::new(dated.clone(), "cli").with_created_at(1_700_000_000_123),
        )
        .expect("create dated");
    store
        .create_session(SessionMetadata::new(undated.clone(), "cli"))
        .expect("create undated");

    assert_eq!(
        store.metadata(&dated).expect("dated").created_at(),
        Some(1_700_000_000_123)
    );
    assert_eq!(
        store.metadata(&undated).expect("undated").created_at(),
        None
    );
}

#[test]
fn title_round_trips_through_the_store() {
    let mut store = StorageBackedSessionStore::new(
        StorageScope::project("org-1", "project-1"),
        InMemoryStorage::default(),
    );
    let session_id = SessionId::from_static("session-titled");

    store
        .create_session(
            SessionMetadata::new(session_id.clone(), "cli").with_title("Review release blockers"),
        )
        .expect("create titled session");

    assert_eq!(
        store.metadata(&session_id).expect("metadata").title(),
        Some("Review release blockers")
    );
}

#[test]
fn set_session_title_updates_and_clears_the_title() {
    let mut store = StorageBackedSessionStore::new(
        StorageScope::project("org-1", "project-1"),
        InMemoryStorage::default(),
    );
    let session_id = SessionId::from_static("session-rename");
    store
        .create_session(SessionMetadata::new(session_id.clone(), "cli"))
        .expect("create session");

    store
        .set_session_title(&session_id, "Renamed conversation")
        .expect("set title");
    assert_eq!(
        store.metadata(&session_id).expect("metadata").title(),
        Some("Renamed conversation")
    );

    // An empty/whitespace title clears it (mirrors `with_title` semantics).
    store
        .set_session_title(&session_id, "   ")
        .expect("clear title");
    assert_eq!(store.metadata(&session_id).expect("metadata").title(), None);
}

#[test]
fn set_session_title_on_unknown_session_fails() {
    let mut store = InMemorySessionStore::default();
    let error = store
        .set_session_title(&SessionId::from_static("session-missing"), "Nope")
        .expect_err("unknown session should fail");
    assert!(matches!(error, SessionStoreError::SessionNotFound { .. }));
}

#[test]
fn metadata_without_created_at_deserializes_to_none() {
    // A session persisted before `created_at` existed has no such field.
    let legacy = serde_json::json!({
        "session_id": "session-legacy",
        "source": "cli",
        "parent_session_id": null
    });
    let metadata: SessionMetadata = serde_json::from_value(legacy).expect("deserialize legacy");
    assert_eq!(metadata.created_at(), None);
    assert_eq!(metadata.title(), None);
    assert_eq!(metadata.session_id().as_str(), "session-legacy");
}
