use codel00p_protocol::{
    AgentEvent, EventId, SessionId, SessionMessage, SessionPersistenceEvent, TurnId,
};
use codel00p_session::{
    InMemorySessionStore, SessionMetadata, SessionRecord, SessionStore, SessionStoreError,
};

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
