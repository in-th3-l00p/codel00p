use super::support::*;
use codel00p_memory::{MemoryEdit, MemoryRepository, StorageBackedMemoryStore};
use codel00p_storage::{SqliteStorage, StorageScope};

fn edit_memory(db_path: &std::path::Path, id: &str, actor: &str, content: &str, reason: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .edit(
            id,
            MemoryEdit::replace_content(actor, content).with_reason(reason),
        )
        .expect("edit memory");
}

#[test]
fn memory_revisions_lists_initial_content_for_new_candidate() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-rev",
        MemoryKind::Workflow,
        "Run tests before pushing.",
        "verify",
    );

    let out = run_codel00p(&db_path, &["memory", "revisions", "mem-rev"]);

    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("1\t"),
        "expected revision 1 in output: {text}"
    );
    assert!(
        text.contains("candidate_created"),
        "expected action in output: {text}"
    );
    assert!(
        text.contains("Run tests"),
        "expected content preview in output: {text}"
    );
}

#[test]
fn memory_revisions_shows_all_content_snapshots_in_order() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-rev",
        MemoryKind::Workflow,
        "Run tests before pushing.",
        "verify",
    );
    approve_candidate(&db_path, "mem-rev", "alice");
    edit_memory(
        &db_path,
        "mem-rev",
        "bob",
        "Run pnpm verify before pushing main.",
        "clarified command",
    );
    edit_memory(
        &db_path,
        "mem-rev",
        "carol",
        "Run pnpm verify && pnpm test before pushing.",
        "added test step",
    );

    let out = run_codel00p(&db_path, &["memory", "revisions", "mem-rev"]);

    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    // Should have 3 revisions
    assert!(text.contains("1\t"), "missing revision 1: {text}");
    assert!(text.contains("2\t"), "missing revision 2: {text}");
    assert!(text.contains("3\t"), "missing revision 3: {text}");
    assert!(text.contains("bob"), "missing actor bob: {text}");
    assert!(text.contains("carol"), "missing actor carol: {text}");
}

#[test]
fn memory_revisions_json_output_contains_full_content() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-rev",
        MemoryKind::Architecture,
        "The harness owns tool execution.",
        "arch",
    );
    approve_candidate(&db_path, "mem-rev", "alice");
    edit_memory(
        &db_path,
        "mem-rev",
        "bob",
        "The harness owns tool execution and streaming.",
        "added streaming",
    );

    let out = run_codel00p(&db_path, &["memory", "revisions", "mem-rev", "--json"]);

    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let items: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("parse revisions json");
    let items = items.as_array().expect("expected array");
    assert_eq!(items.len(), 2);

    assert_eq!(items[0]["revision"], 1);
    assert_eq!(items[0]["action"], "candidate_created");
    assert_eq!(items[0]["actor"], "system");
    assert_eq!(items[0]["content"], "The harness owns tool execution.");

    assert_eq!(items[1]["revision"], 2);
    assert_eq!(items[1]["action"], "edited");
    assert_eq!(items[1]["actor"], "bob");
    assert_eq!(
        items[1]["content"],
        "The harness owns tool execution and streaming."
    );
    assert_eq!(items[1]["reason"], "added streaming");
}

#[test]
fn memory_revisions_unknown_id_prints_error() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    // empty db — no memories seeded

    let out = run_codel00p(&db_path, &["memory", "revisions", "nonexistent"]);

    assert!(!out.status.success(), "expected failure for unknown id");
}
