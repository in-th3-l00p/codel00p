//! Shared fixtures for agent CLI integration tests.

pub(crate) use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

pub(crate) use codel00p_protocol::{SessionId, SessionMessage};
pub(crate) use codel00p_session::{SessionMetadata, SessionStore, StorageBackedSessionStore};
pub(crate) use codel00p_storage::{SqliteStorage, StorageScope};
pub(crate) use httpmock::{Method::POST, MockServer};
pub(crate) use serde_json::json;
pub(crate) use tempfile::tempdir;

pub(crate) fn seed_chat_session(db_path: &Path, id: &'static str, messages: &[SessionMessage]) {
    let storage = SqliteStorage::open(db_path).expect("open sqlite storage");
    let mut store =
        StorageBackedSessionStore::new(StorageScope::project("org-1", "project-1"), storage);
    let session_id = SessionId::from_static(id);
    store
        .create_session(SessionMetadata::new(session_id.clone(), "chat"))
        .expect("create session");
    for message in messages {
        store
            .append_message(&session_id, message.clone())
            .expect("append message");
    }
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
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .output()
        .expect("run codel00p")
}

pub(crate) fn run_codel00p_without_provider_env(db_path: &Path, args: &[&str]) -> Output {
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
        .env_remove("CODEL00P_PROVIDER_CUSTOM_API_KEY")
        .env_remove("CODEL00P_PROVIDER_OPENAI_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .args(args)
        .output()
        .expect("run codel00p")
}

pub(crate) fn run_codel00p_with_env(db_path: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_codel00p"));
    command
        .env("CODEL00P_HOME", db_path.parent().unwrap_or(db_path))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p");
    for (key, value) in env {
        command.env(key, value);
    }
    command.args(args).output().expect("run codel00p")
}

pub(crate) fn run_codel00p_with_stdin(db_path: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn codel00p");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");

    child.wait_with_output().expect("run codel00p")
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

pub(crate) fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}
