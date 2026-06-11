#![cfg(feature = "sqlite")]

use serde_json::json;

use codel00p_storage::{
    AppendLogStore, DocumentStore, KeyValueStore, SqliteStorage, StorageDocument, StorageError,
    StorageScope, StorageValue,
};

fn temp_sqlite_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "codel00p-storage-{name}-{}.sqlite",
        std::process::id()
    ))
}

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
fn sqlite_missing_reads_return_none_or_empty() {
    let storage = SqliteStorage::in_memory().expect("open sqlite storage");
    let scope = StorageScope::workspace("workspace-1");

    assert!(
        storage
            .get_value(&scope, "missing")
            .expect("get value")
            .is_none()
    );
    assert!(
        storage
            .get_document(&scope, "memory", "missing")
            .expect("get document")
            .is_none()
    );
    assert!(
        storage
            .replay_log(&scope, "missing")
            .expect("replay log")
            .is_empty()
    );
}

#[test]
fn sqlite_key_values_increment_versions_and_preserve_metadata() {
    let mut storage = SqliteStorage::in_memory().expect("open sqlite storage");
    let scope = StorageScope::user("user-1");

    let first = storage
        .put_value(
            StorageValue::new(scope.clone(), "provider.selected", json!("openai"))
                .with_metadata("source", "cli"),
        )
        .expect("put first value");
    let second = storage
        .put_value(
            StorageValue::new(scope.clone(), "provider.selected", json!("anthropic"))
                .with_metadata("source", "cloud"),
        )
        .expect("put second value");
    let loaded = storage
        .get_value(&scope, "provider.selected")
        .expect("get value")
        .expect("stored value");

    assert_eq!(first.version(), 1);
    assert_eq!(second.version(), 2);
    assert_eq!(loaded.version(), 2);
    assert_eq!(loaded.payload(), &json!("anthropic"));
    assert_eq!(
        loaded.metadata().get("source").map(String::as_str),
        Some("cloud")
    );
}

#[test]
fn sqlite_documents_increment_versions_and_preserve_metadata() {
    let mut storage = SqliteStorage::in_memory().expect("open sqlite storage");
    let scope = StorageScope::project("org-1", "project-1");

    let first = storage
        .put_document(
            StorageDocument::new(scope.clone(), "memory", "entry-1", json!({ "text": "old" }))
                .with_metadata("author", "agent"),
        )
        .expect("put first document");
    let second = storage
        .put_document(
            StorageDocument::new(scope.clone(), "memory", "entry-1", json!({ "text": "new" }))
                .with_metadata("author", "human"),
        )
        .expect("put second document");
    let loaded = storage
        .get_document(&scope, "memory", "entry-1")
        .expect("get document")
        .expect("stored document");

    assert_eq!(first.version(), 1);
    assert_eq!(second.version(), 2);
    assert_eq!(loaded.version(), 2);
    assert_eq!(loaded.payload(), &json!({ "text": "new" }));
    assert_eq!(
        loaded.metadata().get("author").map(String::as_str),
        Some("human")
    );
}

#[test]
fn sqlite_isolates_scope_collection_id_and_streams() {
    let mut storage = SqliteStorage::in_memory().expect("open sqlite storage");
    let global_scope = StorageScope::global();
    let project_scope = StorageScope::project("org-1", "project-1");
    let workspace_scope = StorageScope::workspace("workspace-1");
    let user_scope = StorageScope::user("user-1");

    storage
        .put_value(StorageValue::new(
            global_scope.clone(),
            "same-key",
            json!("global"),
        ))
        .expect("put global value");
    storage
        .put_value(StorageValue::new(
            project_scope.clone(),
            "same-key",
            json!("project"),
        ))
        .expect("put project value");
    storage
        .put_document(StorageDocument::new(
            project_scope.clone(),
            "settings",
            "same-id",
            json!("settings"),
        ))
        .expect("put settings document");
    storage
        .put_document(StorageDocument::new(
            project_scope.clone(),
            "memory",
            "same-id",
            json!("memory"),
        ))
        .expect("put memory document");
    storage
        .append_log(workspace_scope.clone(), "session/one", json!("workspace"))
        .expect("append workspace log");
    storage
        .append_log(user_scope.clone(), "session/one", json!("user"))
        .expect("append user log");
    storage
        .append_log(
            workspace_scope.clone(),
            "session/two",
            json!("other stream"),
        )
        .expect("append other stream");

    assert_eq!(
        storage
            .get_value(&global_scope, "same-key")
            .expect("get global")
            .expect("global value")
            .payload(),
        &json!("global")
    );
    assert_eq!(
        storage
            .get_value(&project_scope, "same-key")
            .expect("get project")
            .expect("project value")
            .payload(),
        &json!("project")
    );
    assert_eq!(
        storage
            .get_document(&project_scope, "settings", "same-id")
            .expect("get settings")
            .expect("settings document")
            .payload(),
        &json!("settings")
    );
    assert_eq!(
        storage
            .get_document(&project_scope, "memory", "same-id")
            .expect("get memory")
            .expect("memory document")
            .payload(),
        &json!("memory")
    );
    assert_eq!(
        storage
            .replay_log(&workspace_scope, "session/one")
            .expect("replay workspace")[0]
            .payload(),
        &json!("workspace")
    );
    assert_eq!(
        storage
            .replay_log(&user_scope, "session/one")
            .expect("replay user")[0]
            .payload(),
        &json!("user")
    );
    assert_eq!(
        storage
            .replay_log(&workspace_scope, "session/two")
            .expect("replay other stream")[0]
            .payload(),
        &json!("other stream")
    );
}

#[test]
fn sqlite_key_values_can_be_listed_and_deleted_by_scope() {
    let mut storage = SqliteStorage::in_memory().expect("open sqlite storage");
    let first_scope = StorageScope::project("org-1", "project-1");
    let second_scope = StorageScope::project("org-1", "project-2");

    storage
        .put_value(StorageValue::new(
            first_scope.clone(),
            "connector_permission:external_connector:mcp.memory.list",
            json!({ "status": "allow" }),
        ))
        .expect("put first value");
    storage
        .put_value(StorageValue::new(
            first_scope.clone(),
            "connector_permission:external_connector:mcp.memory.show",
            json!({ "status": "deny" }),
        ))
        .expect("put second value");
    storage
        .put_value(StorageValue::new(
            second_scope.clone(),
            "connector_permission:external_connector:mcp.memory.list",
            json!({ "status": "allow" }),
        ))
        .expect("put other scope value");

    let values = storage
        .list_values(&first_scope, Some("connector_permission:"))
        .expect("list values");
    let keys = values.iter().map(|value| value.key()).collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec![
            "connector_permission:external_connector:mcp.memory.list",
            "connector_permission:external_connector:mcp.memory.show"
        ]
    );
    assert!(
        storage
            .delete_value(&first_scope, keys[0])
            .expect("delete value")
    );
    assert!(
        !storage
            .delete_value(&first_scope, keys[0])
            .expect("delete missing")
    );
    assert_eq!(
        storage
            .list_values(&first_scope, Some("connector_permission:"))
            .expect("list after delete")
            .len(),
        1
    );
    assert!(
        storage
            .get_value(
                &second_scope,
                "connector_permission:external_connector:mcp.memory.list"
            )
            .expect("get other scope")
            .is_some()
    );
}

#[test]
fn sqlite_persists_across_reopened_file_connections() {
    let path = temp_sqlite_path("reopen");
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

#[test]
fn sqlite_append_log_sequence_continues_across_reopened_connections() {
    let path = temp_sqlite_path("sequence");
    let scope = StorageScope::project("org-1", "project-1");

    {
        let mut storage = SqliteStorage::open(&path).expect("open sqlite storage");
        storage
            .append_log(scope.clone(), "session/session-1", json!("first"))
            .expect("append first");
    }

    let mut storage = SqliteStorage::open(&path).expect("reopen sqlite storage");
    let second = storage
        .append_log(scope.clone(), "session/session-1", json!("second"))
        .expect("append second");
    let replayed = storage
        .replay_log(&scope, "session/session-1")
        .expect("replay log");

    let _ = std::fs::remove_file(path);

    assert_eq!(second.sequence(), 2);
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[1].payload(), &json!("second"));
}

#[test]
fn sqlite_append_log_handles_concurrent_file_connections() {
    let path = temp_sqlite_path("concurrent-append");
    let scope = StorageScope::project("org-1", "project-1");
    let stream = "session/concurrent";

    {
        let _storage = SqliteStorage::open(&path).expect("initialize sqlite storage");
    }

    let mut workers = Vec::new();
    for index in 0..8 {
        let path = path.clone();
        let scope = scope.clone();
        workers.push(std::thread::spawn(move || {
            let mut storage = SqliteStorage::open(&path).expect("open sqlite storage");
            storage
                .append_log(scope, stream, json!({ "worker": index }))
                .expect("append concurrent entry");
        }));
    }

    for worker in workers {
        worker.join().expect("worker should not panic");
    }

    let storage = SqliteStorage::open(&path).expect("reopen sqlite storage");
    let replayed = storage.replay_log(&scope, stream).expect("replay log");

    let _ = std::fs::remove_file(path);

    assert_eq!(replayed.len(), 8);
    assert_eq!(
        replayed
            .iter()
            .map(|entry| entry.sequence())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 7, 8]
    );
}

#[test]
fn sqlite_open_reports_backend_error_for_directory_path() {
    let result = SqliteStorage::open(std::env::temp_dir());
    let error = match result {
        Ok(_) => panic!("directory is not a db file"),
        Err(error) => error,
    };

    assert!(matches!(error, StorageError::Backend { .. }));
}

#[test]
fn sqlite_reports_serialization_error_for_corrupt_json_payloads() {
    let path = temp_sqlite_path("corrupt-json");
    let scope = StorageScope::workspace("workspace-corrupt");

    {
        let storage = SqliteStorage::open(&path).expect("open sqlite storage");
        drop(storage);
    }

    let connection = rusqlite::Connection::open(&path).expect("open raw sqlite connection");
    connection
        .execute(
            "
            INSERT INTO storage_values (scope, key, version, payload, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            rusqlite::params![
                serde_json::to_string(&scope).expect("serialize scope"),
                "bad-json",
                1_u64,
                "{not-json",
                "{}"
            ],
        )
        .expect("insert corrupt value");
    drop(connection);

    let storage = SqliteStorage::open(&path).expect("reopen sqlite storage");
    let error = storage
        .get_value(&scope, "bad-json")
        .expect_err("corrupt json should fail");

    let _ = std::fs::remove_file(path);

    assert!(matches!(error, StorageError::Serialization { .. }));
}

#[test]
fn sqlite_documents_can_be_listed_by_scope_and_collection() {
    let path = temp_sqlite_path("list-documents");
    let scope = StorageScope::project("org-1", "project-1");
    let other_scope = StorageScope::project("org-1", "project-2");

    let mut storage = SqliteStorage::open(&path).expect("open sqlite storage");
    storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "sessions",
            "session-2",
            json!({ "source": "cli" }),
        ))
        .expect("put document");
    storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "sessions",
            "session-1",
            json!({ "source": "cli" }),
        ))
        .expect("put document");
    storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "memory",
            "entry-1",
            json!({ "text": "ignore me" }),
        ))
        .expect("put document");
    storage
        .put_document(StorageDocument::new(
            other_scope,
            "sessions",
            "session-9",
            json!({ "source": "cli" }),
        ))
        .expect("put document");

    let sessions = storage
        .list_documents(&scope, "sessions")
        .expect("list documents");
    let ids: Vec<&str> = sessions.iter().map(StorageDocument::id).collect();

    let _ = std::fs::remove_file(path);

    assert_eq!(ids, ["session-1", "session-2"]);
}
