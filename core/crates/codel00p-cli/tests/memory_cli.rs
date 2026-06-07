use std::{
    path::Path,
    process::{Command, Output},
};

use codel00p_memory::{
    MemoryCandidateInput, MemoryListFilter, MemoryRepository, StorageBackedMemoryStore,
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

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "mem-workflow\tcandidate\tworkflow\tRun pnpm verify before pushing main.\n"
    );
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
    let audit = run_codel00p(&db_path, &["memory", "audit", "mem-workflow"]);

    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert!(audit.status.success(), "stderr: {}", stderr(&audit));
    assert_eq!(
        stdout(&show),
        "id: mem-workflow\nstatus: candidate\nkind: workflow\ntags: verify\ncontent: Run pnpm verify before pushing main.\n"
    );
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
        &["memory", "approve", "mem-workflow", "--actor", "alice"],
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
        ],
    );

    assert!(approve.status.success(), "stderr: {}", stderr(&approve));
    assert!(archive.status.success(), "stderr: {}", stderr(&archive));
    assert_eq!(stdout(&approve), "mem-workflow\tapproved\n");
    assert_eq!(stdout(&archive), "mem-workflow\tarchived\n");

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
