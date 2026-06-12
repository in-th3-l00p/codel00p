#![cfg(feature = "postgres")]

//! Contract tests for the Postgres backend. They are skipped unless
//! `CODEL00P_TEST_DATABASE_URL` points at a reachable Postgres, mirroring the
//! repo's default-off live-test convention. Each test isolates itself under a
//! unique organization scope so runs are repeatable against a shared database.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use codel00p_storage::{
    AppendLogStore, DocumentStore, KeyValueStore, PostgresStorage, StorageDocument, StorageScope,
    StorageValue,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn database_url() -> Option<String> {
    std::env::var("CODEL00P_TEST_DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// A fresh, unique scope so version/sequence assertions start from a clean slate
/// no matter how many times the suite runs against the same database.
fn unique_scope() -> StorageScope {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    StorageScope::project(format!("org-{}-{id}", std::process::id()), "project-1")
}

fn connect() -> Option<PostgresStorage> {
    let url = database_url()?;
    Some(PostgresStorage::connect(&url).expect("connect to postgres"))
}

#[test]
fn postgres_round_trips_all_primitives() {
    let Some(mut storage) = connect() else {
        return;
    };
    let scope = unique_scope();

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
            json!({ "text": "first" }),
        )
        .expect("append first");
    storage
        .append_log(
            scope.clone(),
            "session/session-1",
            json!({ "text": "second" }),
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
    assert_eq!(replayed[1].payload(), &json!({ "text": "second" }));
}

#[test]
fn postgres_missing_reads_return_none_or_empty() {
    let Some(storage) = connect() else {
        return;
    };
    let scope = unique_scope();

    assert!(storage.get_value(&scope, "missing").expect("get").is_none());
    assert!(
        storage
            .get_document(&scope, "memory", "missing")
            .expect("get")
            .is_none()
    );
    assert!(
        storage
            .replay_log(&scope, "missing")
            .expect("replay")
            .is_empty()
    );
}

#[test]
fn postgres_increments_versions_and_preserves_metadata() {
    let Some(mut storage) = connect() else {
        return;
    };
    let scope = unique_scope();

    let first = storage
        .put_document(
            StorageDocument::new(scope.clone(), "memory", "entry-1", json!({ "text": "old" }))
                .with_metadata("author", "agent"),
        )
        .expect("put first");
    let second = storage
        .put_document(
            StorageDocument::new(scope.clone(), "memory", "entry-1", json!({ "text": "new" }))
                .with_metadata("author", "human"),
        )
        .expect("put second");
    let loaded = storage
        .get_document(&scope, "memory", "entry-1")
        .expect("get")
        .expect("stored");

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
fn postgres_isolates_scopes_and_lists_and_deletes() {
    let Some(mut storage) = connect() else {
        return;
    };
    let scope = unique_scope();
    let other = unique_scope();

    storage
        .put_value(StorageValue::new(
            scope.clone(),
            "perm:list",
            json!("allow"),
        ))
        .expect("put");
    storage
        .put_value(StorageValue::new(scope.clone(), "perm:show", json!("deny")))
        .expect("put");
    storage
        .put_value(StorageValue::new(
            other.clone(),
            "perm:list",
            json!("allow"),
        ))
        .expect("put other");

    let listed = storage
        .list_values(&scope, Some("perm:"))
        .expect("list values");
    let keys: Vec<&str> = listed.iter().map(StorageValue::key).collect();
    assert_eq!(keys, vec!["perm:list", "perm:show"]);

    assert!(storage.delete_value(&scope, "perm:list").expect("delete"));
    assert!(
        !storage
            .delete_value(&scope, "perm:list")
            .expect("delete missing")
    );
    assert_eq!(
        storage
            .list_values(&scope, Some("perm:"))
            .expect("list")
            .len(),
        1
    );
    // The other scope is untouched.
    assert!(
        storage
            .get_value(&other, "perm:list")
            .expect("get other")
            .is_some()
    );
}
