use serde_json::json;

use codel00p_storage::{
    AppendLogStore, DocumentStore, InMemoryStorage, KeyValueStore, StorageDocument, StorageScope,
    StorageValue,
};

#[test]
fn documents_are_stored_by_scope_collection_and_id() {
    let mut storage = InMemoryStorage::default();
    let scope = StorageScope::project("org-1", "project-1");

    let inserted = storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "sessions",
            "session-1",
            json!({ "source": "cli" }),
        ))
        .expect("put document");

    let loaded = storage
        .get_document(&scope, "sessions", "session-1")
        .expect("get document")
        .expect("stored document");

    assert_eq!(inserted.version(), 1);
    assert_eq!(loaded.payload(), &json!({ "source": "cli" }));
}

#[test]
fn documents_increment_version_when_replaced() {
    let mut storage = InMemoryStorage::default();
    let scope = StorageScope::workspace("workspace-1");

    storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "settings",
            "provider",
            json!({ "model": "first" }),
        ))
        .expect("insert document");
    let replaced = storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "settings",
            "provider",
            json!({ "model": "second" }),
        ))
        .expect("replace document");

    assert_eq!(replaced.version(), 2);
    assert_eq!(replaced.payload(), &json!({ "model": "second" }));
}

#[test]
fn append_log_entries_replay_in_sequence_order() {
    let mut storage = InMemoryStorage::default();
    let scope = StorageScope::project("org-1", "project-1");

    let first = storage
        .append_log(
            scope.clone(),
            "session/session-1",
            json!({ "text": "first" }),
        )
        .expect("append first");
    let second = storage
        .append_log(
            scope.clone(),
            "session/session-1",
            json!({ "text": "second" }),
        )
        .expect("append second");

    let replayed = storage
        .replay_log(&scope, "session/session-1")
        .expect("replay log");

    assert_eq!(first.sequence(), 1);
    assert_eq!(second.sequence(), 2);
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].payload(), &json!({ "text": "first" }));
    assert_eq!(replayed[1].payload(), &json!({ "text": "second" }));
}

#[test]
fn scoped_key_values_are_isolated() {
    let mut storage = InMemoryStorage::default();
    let first_scope = StorageScope::workspace("workspace-1");
    let second_scope = StorageScope::workspace("workspace-2");

    storage
        .put_value(StorageValue::new(
            first_scope.clone(),
            "provider.selected",
            json!("openai"),
        ))
        .expect("put first value");
    storage
        .put_value(StorageValue::new(
            second_scope.clone(),
            "provider.selected",
            json!("anthropic"),
        ))
        .expect("put second value");

    assert_eq!(
        storage
            .get_value(&first_scope, "provider.selected")
            .expect("get first")
            .expect("first value")
            .payload(),
        &json!("openai")
    );
    assert_eq!(
        storage
            .get_value(&second_scope, "provider.selected")
            .expect("get second")
            .expect("second value")
            .payload(),
        &json!("anthropic")
    );
}
