//! Shared JSON-RPC harness for codel00p MCP server integration tests.

pub(crate) use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdout, Command, Stdio},
};

pub(crate) use serde_json::{Value, json};
pub(crate) use tempfile::tempdir;

pub(crate) fn spawn_codel00p_mcp_server(db_path: &Path) -> Child {
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
        .arg("mcp")
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn codel00p mcp server")
}

pub(crate) fn send(child: &mut Child, message: Value) {
    writeln!(
        child.stdin.as_mut().expect("stdin"),
        "{}",
        serde_json::to_string(&message).expect("encode message")
    )
    .expect("write message");
}

pub(crate) fn read_response(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read response");
    assert!(!line.trim().is_empty(), "empty response");
    serde_json::from_str(&line).expect("json response")
}

pub(crate) fn read_message(stdout: &mut BufReader<ChildStdout>) -> Value {
    read_response(stdout)
}
