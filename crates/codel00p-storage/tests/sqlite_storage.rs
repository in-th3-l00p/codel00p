#![cfg(feature = "sqlite")]

use serde_json::json;

use codel00p_storage::{
    AppendLogStore, DocumentStore, KeyValueStore, SqliteStorage, StorageDocument, StorageScope,
    StorageValue,
};

#[test]
fn sqlite_backend_implements_all_storage_primitives() {
    let mut storage = SqliteStorage::in_memory().expect("open sqlite storage");
    let scope = StorageScope::project("org-1", "project-1");

    storage
        .put_value(StorageValue::new(
            scope.clone(),
            "provider.selected",
            json!("github-copilot"),
        ))
        .expect("put value");
    storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "sessions",
            "session-1",
            json!({ "source": "cli" }),
        ))
        .expect("put document");
    storage
        .append_log(
            scope.clone(),
            "session/session-1",
            json!({ "kind": "message", "text": "first" }),
        )
        .expect("append first");
    storage
        .append_log(
            scope.clone(),
            "session/session-1",
            json!({ "kind": "message", "text": "second" }),
        )
        .expect("append second");

    let value = storage
        .get_value(&scope, "provider.selected")
        .expect("get value")
        .expect("stored value");
    let document = storage
        .get_document(&scope, "sessions", "session-1")
        .expect("get document")
        .expect("stored document");
    let replayed = storage
        .replay_log(&scope, "session/session-1")
        .expect("replay log");

    assert_eq!(value.payload(), &json!("github-copilot"));
    assert_eq!(document.payload(), &json!({ "source": "cli" }));
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].sequence(), 1);
    assert_eq!(replayed[1].sequence(), 2);
}

#[test]
fn sqlite_persists_across_reopened_file_connections() {
    let path = std::env::temp_dir().join(format!("codel00p-storage-{}.sqlite", std::process::id()));
    let scope = StorageScope::workspace("workspace-1");

    {
        let mut storage = SqliteStorage::open(&path).expect("open sqlite storage");
        storage
            .put_document(StorageDocument::new(
                scope.clone(),
                "memory",
                "entry-1",
                json!({ "text": "Durable project knowledge" }),
            ))
            .expect("put document");
    }

    let storage = SqliteStorage::open(&path).expect("reopen sqlite storage");
    let document = storage
        .get_document(&scope, "memory", "entry-1")
        .expect("get document")
        .expect("stored document");

    let _ = std::fs::remove_file(path);

    assert_eq!(
        document.payload(),
        &json!({ "text": "Durable project knowledge" })
    );
}
