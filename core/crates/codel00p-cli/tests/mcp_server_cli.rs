use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdout, Command, Stdio},
};

use serde_json::{Value, json};
use tempfile::tempdir;

fn spawn_codel00p_mcp_server(db_path: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
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

fn send(child: &mut Child, message: Value) {
    writeln!(
        child.stdin.as_mut().expect("stdin"),
        "{}",
        serde_json::to_string(&message).expect("encode message")
    )
    .expect("write message");
}

fn read_response(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read response");
    assert!(!line.trim().is_empty(), "empty response");
    serde_json::from_str(&line).expect("json response")
}

fn read_message(stdout: &mut BufReader<ChildStdout>) -> Value {
    read_response(stdout)
}

#[test]
fn mcp_serve_exposes_memory_tools_over_stdio_json_rpc() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let mut child = spawn_codel00p_mcp_server(&db_path);
    let stdout = child.stdout.take().expect("stdout");
    let mut stdout = BufReader::new(stdout);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }),
    );
    let initialize = read_response(&mut stdout);
    assert_eq!(initialize["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(initialize["result"]["serverInfo"]["name"], "codel00p");
    assert_eq!(
        initialize["result"]["capabilities"]["resources"]["subscribe"],
        true
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    );
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    );
    let tools = read_response(&mut stdout);
    let tool_names = tools["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"memory_search"));
    assert!(tool_names.contains(&"memory_list"));
    assert!(tool_names.contains(&"memory_show"));
    assert!(tool_names.contains(&"memory_create_candidate"));
    assert!(tool_names.contains(&"memory_approve"));
    assert!(tool_names.contains(&"memory_reject"));
    assert!(tool_names.contains(&"memory_archive"));
    assert!(tool_names.contains(&"session_show"));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_list",
                "arguments": { "limit": 5 }
            }
        }),
    );
    let call = read_response(&mut stdout);
    assert_eq!(call["result"]["isError"], false);
    assert_eq!(call["result"]["content"][0]["type"], "text");
    assert_eq!(call["result"]["content"][0]["text"], "[]");

    child.kill().expect("kill server");
    let _ = child.wait();
}

#[test]
fn mcp_serve_exposes_memory_and_session_resources() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let mut child = spawn_codel00p_mcp_server(&db_path);
    let stdout = child.stdout.take().expect("stdout");
    let mut stdout = BufReader::new(stdout);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }),
    );
    let _ = read_response(&mut stdout);
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resources/list",
            "params": {}
        }),
    );
    let resources = read_response(&mut stdout);
    let resource_templates = resources["result"]["resourceTemplates"]
        .as_array()
        .expect("resource templates");
    assert!(resource_templates.iter().any(|template| {
        template["uriTemplate"] == "codel00p://memory/{id}"
            && template["mimeType"] == "application/json"
    }));
    assert!(resource_templates.iter().any(|template| {
        template["uriTemplate"] == "codel00p://sessions/{session_id}"
            && template["mimeType"] == "application/json"
    }));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-resource-1",
                    "kind": "architecture",
                    "content": "Expose memory as MCP resources.",
                    "session_id": "session-resource",
                    "turn_id": "turn-resource",
                    "tags": ["mcp"]
                }
            }
        }),
    );
    let created = read_response(&mut stdout);
    assert_eq!(created["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/read",
            "params": {
                "uri": "codel00p://memory/mem-resource-1"
            }
        }),
    );
    let memory = read_response(&mut stdout);
    assert_eq!(
        memory["result"]["contents"][0]["uri"],
        "codel00p://memory/mem-resource-1"
    );
    assert_eq!(
        memory["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    let memory_text = memory["result"]["contents"][0]["text"]
        .as_str()
        .expect("memory text");
    assert!(memory_text.contains(r#""id":"mem-resource-1""#));
    assert!(memory_text.contains(r#""kind":"architecture""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/read",
            "params": {
                "uri": "codel00p://sessions/session-resource"
            }
        }),
    );
    let session = read_response(&mut stdout);
    assert_eq!(
        session["result"]["contents"][0]["uri"],
        "codel00p://sessions/session-resource"
    );
    assert_eq!(
        session["result"]["contents"][0]["mimeType"],
        "application/json"
    );
    assert_eq!(session["result"]["contents"][0]["text"], "[]");

    child.kill().expect("kill server");
    let _ = child.wait();
}

#[test]
fn mcp_serve_notifies_subscribed_memory_resources_after_updates() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let mut child = spawn_codel00p_mcp_server(&db_path);
    let stdout = child.stdout.take().expect("stdout");
    let mut stdout = BufReader::new(stdout);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }),
    );
    let initialize = read_response(&mut stdout);
    assert_eq!(
        initialize["result"]["capabilities"]["resources"]["subscribe"],
        true
    );
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resources/subscribe",
            "params": {
                "uri": "codel00p://memory/mem-notify-1"
            }
        }),
    );
    let subscribed = read_response(&mut stdout);
    assert_eq!(subscribed["result"], json!({}));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-notify-1",
                    "kind": "decision",
                    "content": "Notify MCP clients after memory changes.",
                    "session_id": "session-notify",
                    "turn_id": "turn-notify"
                }
            }
        }),
    );
    let response = read_response(&mut stdout);
    assert_eq!(response["result"]["isError"], false);
    let notification = read_message(&mut stdout);
    assert_eq!(notification["jsonrpc"], "2.0");
    assert_eq!(notification["method"], "notifications/resources/updated");
    assert_eq!(
        notification["params"]["uri"],
        "codel00p://memory/mem-notify-1"
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/unsubscribe",
            "params": {
                "uri": "codel00p://memory/mem-notify-1"
            }
        }),
    );
    let unsubscribed = read_response(&mut stdout);
    assert_eq!(unsubscribed["result"], json!({}));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "memory_approve",
                "arguments": {
                    "id": "mem-notify-1",
                    "actor": "mcp-client"
                }
            }
        }),
    );
    let response_after_unsubscribe = read_response(&mut stdout);
    assert_eq!(response_after_unsubscribe["result"]["isError"], false);

    child.kill().expect("kill server");
    let _ = child.wait();
}

#[test]
fn mcp_serve_can_create_and_review_project_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let mut child = spawn_codel00p_mcp_server(&db_path);
    let stdout = child.stdout.take().expect("stdout");
    let mut stdout = BufReader::new(stdout);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }),
    );
    let _ = read_response(&mut stdout);
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-mcp-1",
                    "kind": "decision",
                    "content": "Use MCP write tools for reviewed project memory.",
                    "session_id": "session-mcp",
                    "turn_id": "turn-mcp",
                    "tags": ["mcp", "memory"]
                }
            }
        }),
    );
    let created = read_response(&mut stdout);
    assert_eq!(created["result"]["isError"], false);
    let created_text = created["result"]["content"][0]["text"]
        .as_str()
        .expect("created text");
    assert!(created_text.contains(r#""id":"mem-mcp-1""#));
    assert!(created_text.contains(r#""status":"candidate""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_approve",
                "arguments": {
                    "id": "mem-mcp-1",
                    "actor": "mcp-client"
                }
            }
        }),
    );
    let approved = read_response(&mut stdout);
    assert_eq!(approved["result"]["isError"], false);
    let approved_text = approved["result"]["content"][0]["text"]
        .as_str()
        .expect("approved text");
    assert!(approved_text.contains(r#""status":"approved""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "memory_search",
                "arguments": { "text": "MCP write tools" }
            }
        }),
    );
    let searched = read_response(&mut stdout);
    assert_eq!(searched["result"]["isError"], false);
    let searched_text = searched["result"]["content"][0]["text"]
        .as_str()
        .expect("searched text");
    assert!(searched_text.contains(r#""id":"mem-mcp-1""#));
    assert!(searched_text.contains("reviewed project memory"));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-mcp-reject",
                    "kind": "workflow",
                    "content": "Reject vague MCP memory.",
                    "session_id": "session-mcp",
                    "turn_id": "turn-mcp",
                    "tags": ["mcp"]
                }
            }
        }),
    );
    let rejected_candidate = read_response(&mut stdout);
    assert_eq!(rejected_candidate["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "memory_reject",
                "arguments": {
                    "id": "mem-mcp-reject",
                    "actor": "mcp-client",
                    "reason": "too vague"
                }
            }
        }),
    );
    let rejected = read_response(&mut stdout);
    assert_eq!(rejected["result"]["isError"], false);
    let rejected_text = rejected["result"]["content"][0]["text"]
        .as_str()
        .expect("rejected text");
    assert!(rejected_text.contains(r#""status":"rejected""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "memory_archive",
                "arguments": {
                    "id": "mem-mcp-1",
                    "actor": "mcp-client",
                    "reason": "superseded"
                }
            }
        }),
    );
    let archived = read_response(&mut stdout);
    assert_eq!(archived["result"]["isError"], false);
    let archived_text = archived["result"]["content"][0]["text"]
        .as_str()
        .expect("archived text");
    assert!(archived_text.contains(r#""status":"archived""#));

    child.kill().expect("kill server");
    let _ = child.wait();
}
