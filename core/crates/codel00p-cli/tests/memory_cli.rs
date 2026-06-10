use std::{
    path::Path,
    process::{Command, Output},
};

use codel00p_memory::{
    MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision,
    StorageBackedMemoryStore,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId};
use codel00p_storage::{SqliteStorage, StorageScope};
use tempfile::tempdir;

fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

fn source() -> MemorySource {
    MemorySource::turn(
        SessionId::from_static("session-cli"),
        TurnId::from_static("turn-cli"),
    )
}

fn seed_candidate(db_path: &Path, id: &str, kind: MemoryKind, content: &str, tag: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .create_candidate(
            MemoryCandidateInput::new(id, project(), kind, content, source()).with_tag(tag),
        )
        .expect("create candidate");
}

fn approve_candidate(db_path: &Path, id: &str, actor: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .review(id, ReviewDecision::approve(actor))
        .expect("approve candidate");
}

fn archive_memory(db_path: &Path, id: &str, actor: &str, reason: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .review(id, ReviewDecision::archive(actor, reason))
        .expect("archive memory");
}

fn run_codel00p(db_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .args(args)
        .output()
        .expect("run codel00p")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

#[test]
fn memory_list_prints_filtered_candidates() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-architecture",
        MemoryKind::Architecture,
        "The harness owns tool execution.",
        "harness",
    );

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "list",
            "--status",
            "candidate",
            "--kind",
            "workflow",
            "--tag",
            "verify",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "list",
            "--status",
            "candidate",
            "--kind",
            "workflow",
            "--tag",
            "verify",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );
    assert_eq!(
        stdout(&output),
        "mem-workflow\tcandidate\tworkflow\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("list json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-workflow");
    assert_eq!(records[0]["status"], "candidate");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(
        records[0]["content"],
        "Run pnpm verify before pushing main."
    );
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
    assert_eq!(records[0]["source"]["session_id"], "session-cli");
    assert_eq!(records[0]["source"]["turn_id"], "turn-cli");
    assert_eq!(records[0]["source_uri"], "codel00p://sessions/session-cli");
}

#[test]
fn memory_search_retrieves_approved_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-candidate",
        MemoryKind::Workflow,
        "Candidate verify reminder.",
        "verify",
    );
    approve_candidate(&db_path, "mem-workflow", "alice");

    let output = run_codel00p(
        &db_path,
        &[
            "memory", "search", "--text", "verify", "--kind", "workflow", "--tag", "verify",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory", "search", "--text", "verify", "--kind", "workflow", "--tag", "verify",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );
    assert_eq!(
        stdout(&output),
        "mem-workflow\tapproved\tworkflow\tmatched kind workflow and tag verify and text verify\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("search json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-workflow");
    assert_eq!(records[0]["status"], "approved");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(
        records[0]["content"],
        "Run pnpm verify before pushing main."
    );
    assert_eq!(
        records[0]["reason"],
        "matched kind workflow and tag verify and text verify"
    );
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
    assert_eq!(records[0]["source"]["session_id"], "session-cli");
    assert_eq!(records[0]["source"]["turn_id"], "turn-cli");
    assert_eq!(records[0]["source_uri"], "codel00p://sessions/session-cli");
}

#[test]
fn memory_similar_scores_active_near_duplicates() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-original",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );
    seed_candidate(
        &db_path,
        "mem-unrelated",
        MemoryKind::Workflow,
        "The harness owns tool execution.",
        "harness",
    );
    seed_candidate(
        &db_path,
        "mem-archived",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main branch.",
        "verify",
    );
    approve_candidate(&db_path, "mem-original", "alice");
    approve_candidate(&db_path, "mem-archived", "alice");
    archive_memory(&db_path, "mem-archived", "alice", "superseded");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "similar",
            "--kind",
            "workflow",
            "--content",
            "Run pnpm verify before pushing to main branch.",
            "--threshold",
            "70",
        ],
    );
    let output_json = run_codel00p(
        &db_path,
        &[
            "memory",
            "similar",
            "--kind",
            "workflow",
            "--content",
            "Run pnpm verify before pushing to main branch.",
            "--threshold",
            "70",
            "--json",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        output_json.status.success(),
        "stderr: {}",
        stderr(&output_json)
    );
    assert_eq!(
        stdout(&output),
        "mem-original\tapproved\tworkflow\t75\tRun pnpm verify before pushing main.\n"
    );
    let records: serde_json::Value =
        serde_json::from_str(&stdout(&output_json)).expect("similar json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["id"], "mem-original");
    assert_eq!(records[0]["status"], "approved");
    assert_eq!(records[0]["kind"], "workflow");
    assert_eq!(records[0]["score"], 75);
    assert_eq!(
        records[0]["content"],
        "Run pnpm verify before pushing main."
    );
    assert_eq!(records[0]["tags"], serde_json::json!(["verify"]));
    assert_eq!(records[0]["source_uri"], "codel00p://sessions/session-cli");
}

#[test]
fn memory_show_and_audit_print_stable_details() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let show = run_codel00p(&db_path, &["memory", "show", "mem-workflow"]);
    let show_json = run_codel00p(&db_path, &["memory", "show", "mem-workflow", "--json"]);
    let audit = run_codel00p(&db_path, &["memory", "audit", "mem-workflow"]);

    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert!(show_json.status.success(), "stderr: {}", stderr(&show_json));
    assert!(audit.status.success(), "stderr: {}", stderr(&audit));
    assert_eq!(
        stdout(&show),
        "id: mem-workflow\nstatus: candidate\nkind: workflow\ntags: verify\nsource_session: session-cli\nsource_turn: turn-cli\nsource_uri: codel00p://sessions/session-cli\ncontent: Run pnpm verify before pushing main.\n"
    );
    let record: serde_json::Value = serde_json::from_str(&stdout(&show_json)).expect("show json");
    assert_eq!(record["id"], "mem-workflow");
    assert_eq!(record["status"], "candidate");
    assert_eq!(record["kind"], "workflow");
    assert_eq!(record["content"], "Run pnpm verify before pushing main.");
    assert_eq!(record["tags"], serde_json::json!(["verify"]));
    assert_eq!(record["source"]["session_id"], "session-cli");
    assert_eq!(record["source"]["turn_id"], "turn-cli");
    assert_eq!(record["source_uri"], "codel00p://sessions/session-cli");
    assert_eq!(stdout(&audit), "1\tcandidate_created\tsystem\t\n");
}

#[test]
fn memory_review_commands_persist_state_across_invocations() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let approve = run_codel00p(
        &db_path,
        &[
            "memory",
            "approve",
            "mem-workflow",
            "--actor",
            "alice",
            "--json",
        ],
    );
    let archive = run_codel00p(
        &db_path,
        &[
            "memory",
            "archive",
            "mem-workflow",
            "--actor",
            "bob",
            "--reason",
            "obsolete",
            "--json",
        ],
    );

    assert!(approve.status.success(), "stderr: {}", stderr(&approve));
    assert!(archive.status.success(), "stderr: {}", stderr(&archive));
    let approved: serde_json::Value =
        serde_json::from_str(&stdout(&approve)).expect("approve json");
    assert_eq!(approved["id"], "mem-workflow");
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["kind"], "workflow");
    assert_eq!(approved["content"], "Run pnpm verify before pushing main.");
    assert_eq!(approved["tags"], serde_json::json!(["verify"]));
    assert_eq!(approved["source"]["session_id"], "session-cli");
    assert_eq!(approved["source"]["turn_id"], "turn-cli");
    assert_eq!(approved["source_uri"], "codel00p://sessions/session-cli");

    let archived: serde_json::Value =
        serde_json::from_str(&stdout(&archive)).expect("archive json");
    assert_eq!(archived["id"], "mem-workflow");
    assert_eq!(archived["status"], "archived");
    assert_eq!(archived["kind"], "workflow");
    assert_eq!(archived["content"], "Run pnpm verify before pushing main.");
    assert_eq!(archived["source_uri"], "codel00p://sessions/session-cli");

    let storage = SqliteStorage::open(&db_path).expect("reopen sqlite storage");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let listed = store
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Archived))
        .expect("list archived memory");
    let audit = store.audit_log("mem-workflow").expect("audit");

    assert_eq!(listed.len(), 1);
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[2].reason(), Some("obsolete"));
}

#[test]
fn memory_edit_updates_content_and_prints_audit_event() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run tests before pushing.",
        "verify",
    );
    approve_candidate(&db_path, "mem-workflow", "alice");

    let edit = run_codel00p(
        &db_path,
        &[
            "memory",
            "edit",
            "mem-workflow",
            "--actor",
            "bob",
            "--content",
            "Run pnpm verify before pushing main.",
            "--reason",
            "clarified command",
            "--json",
        ],
    );
    let show = run_codel00p(&db_path, &["memory", "show", "mem-workflow"]);
    let audit = run_codel00p(&db_path, &["memory", "audit", "mem-workflow"]);
    let audit_json = run_codel00p(&db_path, &["memory", "audit", "mem-workflow", "--json"]);

    assert!(edit.status.success(), "stderr: {}", stderr(&edit));
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert!(audit.status.success(), "stderr: {}", stderr(&audit));
    assert!(
        audit_json.status.success(),
        "stderr: {}",
        stderr(&audit_json)
    );
    let edited: serde_json::Value = serde_json::from_str(&stdout(&edit)).expect("edit json");
    assert_eq!(edited["id"], "mem-workflow");
    assert_eq!(edited["status"], "approved");
    assert_eq!(edited["kind"], "workflow");
    assert_eq!(edited["content"], "Run pnpm verify before pushing main.");
    assert_eq!(edited["source_uri"], "codel00p://sessions/session-cli");
    assert_eq!(
        stdout(&show),
        "id: mem-workflow\nstatus: approved\nkind: workflow\ntags: verify\nsource_session: session-cli\nsource_turn: turn-cli\nsource_uri: codel00p://sessions/session-cli\ncontent: Run pnpm verify before pushing main.\n"
    );
    assert_eq!(
        stdout(&audit),
        "1\tcandidate_created\tsystem\t\n2\tapproved\talice\t\n3\tedited\tbob\tclarified command\n"
    );
    let audit_events: serde_json::Value =
        serde_json::from_str(&stdout(&audit_json)).expect("audit json");
    let audit_events = audit_events.as_array().expect("audit array");
    for event in audit_events {
        assert_eq!(event["memory_id"], "mem-workflow");
    }
    let edit_event = &audit_events[2];
    assert_eq!(edit_event["sequence"], 3);
    assert_eq!(edit_event["action"], "edited");
    assert_eq!(edit_event["actor"], "bob");
    assert_eq!(edit_event["reason"], "clarified command");
    assert_eq!(edit_event["previous_content"], "Run tests before pushing.");
    assert_eq!(
        edit_event["new_content"],
        "Run pnpm verify before pushing main."
    );
}

#[test]
fn memory_restore_reverts_to_previous_edit_content() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run tests before pushing.",
        "verify",
    );
    approve_candidate(&db_path, "mem-workflow", "alice");

    let edit = run_codel00p(
        &db_path,
        &[
            "memory",
            "edit",
            "mem-workflow",
            "--actor",
            "bob",
            "--content",
            "Run pnpm verify before pushing main.",
            "--reason",
            "clarified command",
        ],
    );
    let restore = run_codel00p(
        &db_path,
        &[
            "memory",
            "restore",
            "mem-workflow",
            "--sequence",
            "3",
            "--actor",
            "carol",
            "--reason",
            "undo edit",
            "--json",
        ],
    );
    let show = run_codel00p(&db_path, &["memory", "show", "mem-workflow"]);
    let audit_json = run_codel00p(&db_path, &["memory", "audit", "mem-workflow", "--json"]);

    assert!(edit.status.success(), "stderr: {}", stderr(&edit));
    assert!(restore.status.success(), "stderr: {}", stderr(&restore));
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert!(
        audit_json.status.success(),
        "stderr: {}",
        stderr(&audit_json)
    );
    let restored: serde_json::Value =
        serde_json::from_str(&stdout(&restore)).expect("restore json");
    assert_eq!(restored["id"], "mem-workflow");
    assert_eq!(restored["status"], "approved");
    assert_eq!(restored["kind"], "workflow");
    assert_eq!(restored["content"], "Run tests before pushing.");
    assert_eq!(restored["source_uri"], "codel00p://sessions/session-cli");
    assert_eq!(
        stdout(&show),
        "id: mem-workflow\nstatus: approved\nkind: workflow\ntags: verify\nsource_session: session-cli\nsource_turn: turn-cli\nsource_uri: codel00p://sessions/session-cli\ncontent: Run tests before pushing.\n"
    );
    let audit_events: serde_json::Value =
        serde_json::from_str(&stdout(&audit_json)).expect("audit json");
    let audit_events = audit_events.as_array().expect("audit array");
    assert_eq!(audit_events.len(), 4);
    for event in audit_events {
        assert_eq!(event["memory_id"], "mem-workflow");
    }
    let restore_event = &audit_events[3];
    assert_eq!(restore_event["sequence"], 4);
    assert_eq!(restore_event["action"], "edited");
    assert_eq!(restore_event["actor"], "carol");
    assert_eq!(restore_event["reason"], "undo edit");
    assert_eq!(
        restore_event["previous_content"],
        "Run pnpm verify before pushing main."
    );
    assert_eq!(restore_event["new_content"], "Run tests before pushing.");
}

#[test]
fn memory_reject_requires_reason() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &["memory", "reject", "mem-workflow", "--actor", "alice"],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("missing required --reason"));
}

#[test]
fn memory_edit_requires_content() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_candidate(
        &db_path,
        "mem-workflow",
        MemoryKind::Workflow,
        "Run pnpm verify before pushing main.",
        "verify",
    );

    let output = run_codel00p(
        &db_path,
        &["memory", "edit", "mem-workflow", "--actor", "alice"],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("missing required --content"));
}
