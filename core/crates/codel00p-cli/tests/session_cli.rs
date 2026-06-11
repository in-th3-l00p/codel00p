use std::{
    path::Path,
    process::{Command, Output},
};

use codel00p_protocol::{SessionId, SessionMessage};
use codel00p_session::{SessionMetadata, SessionStore, StorageBackedSessionStore};
use codel00p_storage::{SqliteStorage, StorageScope};
use tempfile::tempdir;

fn run_codel00p(db_path: &Path, args: &[&str]) -> Output {
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn seed_session(db_path: &Path, id: &'static str, source: &str, user_messages: &[&str]) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedSessionStore::new(StorageScope::project("org-1", "project-1"), storage);
    let session_id = SessionId::from_static(id);
    store
        .create_session(SessionMetadata::new(session_id.clone(), source))
        .expect("create session");
    for message in user_messages {
        store
            .append_message(&session_id, SessionMessage::user(*message))
            .expect("append message");
    }
}

#[test]
fn session_list_prints_all_conversations() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_session(&db_path, "session-a", "chat", &["hello", "again"]);
    seed_session(&db_path, "session-b", "cli", &["one"]);

    let output = run_codel00p(&db_path, &["session", "list"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let listing = stdout(&output);
    assert!(
        listing.contains("session-a\tchat\t2 message(s)"),
        "stdout: {listing}"
    );
    assert!(
        listing.contains("session-b\tcli\t1 message(s)"),
        "stdout: {listing}"
    );
    // Sorted by id: session-a appears before session-b.
    let a = listing.find("session-a").expect("session-a present");
    let b = listing.find("session-b").expect("session-b present");
    assert!(a < b, "expected session-a before session-b: {listing}");
}

#[test]
fn session_list_json_reports_counts() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    seed_session(&db_path, "session-a", "chat", &["hello", "again"]);

    let output = run_codel00p(&db_path, &["session", "list", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let records: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("list json");
    let records = records.as_array().expect("record array");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["session_id"], "session-a");
    assert_eq!(records[0]["source"], "chat");
    assert_eq!(records[0]["message_count"], 2);
}

#[test]
fn session_list_is_empty_without_sessions() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let output = run_codel00p(&db_path, &["session", "list"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn session_list_rejects_unknown_flags() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let output = run_codel00p(&db_path, &["session", "list", "--nope"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown session list option: --nope"));
}
