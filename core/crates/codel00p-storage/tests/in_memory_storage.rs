use serde_json::json;

use codel00p_storage::{
    AppendLogStore, DocumentStore, InMemoryStorage, KeyValueStore, StorageDocument, StorageScope,
    StorageValue,
};

#[test]
fn scope_constructors_and_accessors_are_stable() {
    let global = StorageScope::global();
    let project = StorageScope::project("org-1", "project-1");
    let workspace = StorageScope::workspace("workspace-1");
    let user = StorageScope::user("user-1");

    assert_eq!(global.organization_id(), None);
    assert_eq!(project.organization_id(), Some("org-1"));
    assert_eq!(project.project_id(), Some("project-1"));
    assert_eq!(workspace.workspace_id(), Some("workspace-1"));
    assert_eq!(user.user_id(), Some("user-1"));
}

#[test]
fn scope_serialization_round_trips() {
    let scope = StorageScope::project("org-1", "project-1");

    let encoded = serde_json::to_string(&scope).expect("serialize scope");
    let decoded: StorageScope = serde_json::from_str(&encoded).expect("deserialize scope");

    assert_eq!(decoded, scope);
}

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
fn missing_documents_and_values_return_none() {
    let storage = InMemoryStorage::default();
    let scope = StorageScope::workspace("workspace-1");

    assert!(
        storage
            .get_document(&scope, "sessions", "missing")
            .expect("get document")
            .is_none()
    );
    assert!(
        storage
            .get_value(&scope, "missing")
            .expect("get value")
            .is_none()
    );
}

#[test]
fn document_metadata_round_trips() {
    let mut storage = InMemoryStorage::default();
    let scope = StorageScope::project("org-1", "project-1");

    storage
        .put_document(
            StorageDocument::new(
                scope.clone(),
                "memory",
                "entry-1",
                json!({ "text": "keep" }),
            )
            .with_metadata("author", "agent")
            .with_metadata("review", "approved"),
        )
        .expect("put document");

    let loaded = storage
        .get_document(&scope, "memory", "entry-1")
        .expect("get document")
        .expect("stored document");

    assert_eq!(
        loaded.metadata().get("author").map(String::as_str),
        Some("agent")
    );
    assert_eq!(
        loaded.metadata().get("review").map(String::as_str),
        Some("approved")
    );
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
fn documents_are_isolated_by_scope_collection_and_id() {
    let mut storage = InMemoryStorage::default();
    let project_scope = StorageScope::project("org-1", "project-1");
    let user_scope = StorageScope::user("user-1");

    storage
        .put_document(StorageDocument::new(
            project_scope.clone(),
            "settings",
            "same-id",
            json!({ "owner": "project" }),
        ))
        .expect("put project document");
    storage
        .put_document(StorageDocument::new(
            user_scope.clone(),
            "settings",
            "same-id",
            json!({ "owner": "user" }),
        ))
        .expect("put user document");
    storage
        .put_document(StorageDocument::new(
            project_scope.clone(),
            "memory",
            "same-id",
            json!({ "owner": "memory" }),
        ))
        .expect("put collection document");

    assert_eq!(
        storage
            .get_document(&project_scope, "settings", "same-id")
            .expect("get project")
            .expect("project document")
            .payload(),
        &json!({ "owner": "project" })
    );
    assert_eq!(
        storage
            .get_document(&user_scope, "settings", "same-id")
            .expect("get user")
            .expect("user document")
            .payload(),
        &json!({ "owner": "user" })
    );
    assert_eq!(
        storage
            .get_document(&project_scope, "memory", "same-id")
            .expect("get memory")
            .expect("memory document")
            .payload(),
        &json!({ "owner": "memory" })
    );
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
    assert_eq!(first.scope(), &scope);
    assert_eq!(second.sequence(), 2);
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].payload(), &json!({ "text": "first" }));
    assert_eq!(replayed[1].payload(), &json!({ "text": "second" }));
}

#[test]
fn missing_append_log_replays_empty() {
    let storage = InMemoryStorage::default();
    let scope = StorageScope::global();

    let replayed = storage
        .replay_log(&scope, "missing")
        .expect("replay missing log");

    assert!(replayed.is_empty());
}

#[test]
fn append_logs_are_isolated_by_scope_and_stream() {
    let mut storage = InMemoryStorage::default();
    let first_scope = StorageScope::workspace("workspace-1");
    let second_scope = StorageScope::workspace("workspace-2");

    storage
        .append_log(first_scope.clone(), "session/one", json!("first"))
        .expect("append first scope");
    storage
        .append_log(second_scope.clone(), "session/one", json!("second"))
        .expect("append second scope");
    storage
        .append_log(first_scope.clone(), "session/two", json!("other stream"))
        .expect("append second stream");

    assert_eq!(
        storage
            .replay_log(&first_scope, "session/one")
            .expect("replay first scope")[0]
            .payload(),
        &json!("first")
    );
    assert_eq!(
        storage
            .replay_log(&second_scope, "session/one")
            .expect("replay second scope")[0]
            .payload(),
        &json!("second")
    );
    assert_eq!(
        storage
            .replay_log(&first_scope, "session/two")
            .expect("replay second stream")[0]
            .payload(),
        &json!("other stream")
    );
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

#[test]
fn key_value_versions_and_metadata_round_trip() {
    let mut storage = InMemoryStorage::default();
    let scope = StorageScope::workspace("workspace-1");

    let inserted = storage
        .put_value(
            StorageValue::new(scope.clone(), "sync.cursor", json!("first"))
                .with_metadata("source", "local"),
        )
        .expect("insert value");
    let replaced = storage
        .put_value(
            StorageValue::new(scope.clone(), "sync.cursor", json!("second"))
                .with_metadata("source", "cloud"),
        )
        .expect("replace value");
    let loaded = storage
        .get_value(&scope, "sync.cursor")
        .expect("get value")
        .expect("stored value");

    assert_eq!(inserted.version(), 1);
    assert_eq!(replaced.version(), 2);
    assert_eq!(loaded.payload(), &json!("second"));
    assert_eq!(
        loaded.metadata().get("source").map(String::as_str),
        Some("cloud")
    );
}

#[test]
fn key_values_can_be_listed_and_deleted_by_scope() {
    let mut storage = InMemoryStorage::default();
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
fn crate_identity_is_exposed() {
    assert_eq!(codel00p_storage::crate_name(), "codel00p-storage");
}
