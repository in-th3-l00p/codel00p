//! Shared memory-store fixtures for memory CLI integration tests.

pub(crate) use std::{
    path::Path,
    process::{Command, Output},
};

pub(crate) use codel00p_memory::{
    MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision,
    StorageBackedMemoryStore,
};
pub(crate) use codel00p_protocol::{
    MemoryKind, MemorySensitivity, MemorySource, MemoryStatus, ProjectRef, SessionId, TurnId,
};
pub(crate) use codel00p_storage::{SqliteStorage, StorageScope};
pub(crate) use tempfile::tempdir;

pub(crate) fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

pub(crate) fn source() -> MemorySource {
    MemorySource::turn(
        SessionId::from_static("session-cli"),
        TurnId::from_static("turn-cli"),
    )
}

pub(crate) fn seed_candidate(db_path: &Path, id: &str, kind: MemoryKind, content: &str, tag: &str) {
    seed_candidate_with_sensitivity(db_path, id, kind, content, tag, MemorySensitivity::Normal);
}

pub(crate) fn seed_candidate_with_sensitivity(
    db_path: &Path,
    id: &str,
    kind: MemoryKind,
    content: &str,
    tag: &str,
    sensitivity: MemorySensitivity,
) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .create_candidate(
            MemoryCandidateInput::new(id, project(), kind, content, source())
                .with_tag(tag)
                .with_sensitivity(sensitivity),
        )
        .expect("create candidate");
}

pub(crate) fn seed_candidate_with_source(
    db_path: &Path,
    id: &str,
    kind: MemoryKind,
    content: &str,
    tag: &str,
    source: MemorySource,
) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .create_candidate(
            MemoryCandidateInput::new(id, project(), kind, content, source).with_tag(tag),
        )
        .expect("create candidate");
}

pub(crate) fn approve_candidate(db_path: &Path, id: &str, actor: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .review(id, ReviewDecision::approve(actor))
        .expect("approve candidate");
}

pub(crate) fn archive_memory(db_path: &Path, id: &str, actor: &str, reason: &str) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    store
        .review(id, ReviewDecision::archive(actor, reason))
        .expect("archive memory");
}

pub(crate) fn run_codel00p(db_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", db_path.parent().unwrap_or(db_path))
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

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}
