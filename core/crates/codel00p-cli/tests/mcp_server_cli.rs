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
    assert!(tool_names.contains(&"memory_similar"));
    assert!(tool_names.contains(&"memory_stale"));
    assert!(tool_names.contains(&"memory_quality"));
    assert!(tool_names.contains(&"memory_search"));
    assert!(tool_names.contains(&"memory_list"));
    assert!(tool_names.contains(&"memory_show"));
    assert!(tool_names.contains(&"memory_create_candidate"));
    assert!(tool_names.contains(&"memory_approve"));
    assert!(tool_names.contains(&"memory_reject"));
    assert!(tool_names.contains(&"memory_archive"));
    assert!(tool_names.contains(&"memory_edit"));
    assert!(tool_names.contains(&"memory_restore"));
    assert!(tool_names.contains(&"memory_audit"));
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
fn mcp_serve_exposes_quality_review_queue() {
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
                    "id": "mem-quality-vague",
                    "kind": "decision",
                    "content": "This is important.",
                    "session_id": "session-quality",
                    "turn_id": "turn-quality"
                }
            }
        }),
    );
    let vague = read_response(&mut stdout);
    assert_eq!(vague["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-quality-strong",
                    "kind": "workflow",
                    "content": "Run pnpm verify before pushing main after editing provider policy.",
                    "session_id": "session-quality",
                    "turn_id": "turn-quality"
                }
            }
        }),
    );
    let strong = read_response(&mut stdout);
    assert_eq!(strong["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-quality-workflow",
                    "kind": "workflow",
                    "content": "Run tests.",
                    "session_id": "session-quality",
                    "turn_id": "turn-quality"
                }
            }
        }),
    );
    let workflow = read_response(&mut stdout);
    assert_eq!(workflow["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "memory_quality",
                "arguments": {
                    "kind": "workflow",
                    "max_score": 80,
                    "limit": 5
                }
            }
        }),
    );
    let quality = read_response(&mut stdout);
    assert_eq!(quality["result"]["isError"], false);
    let quality_text = quality["result"]["content"][0]["text"]
        .as_str()
        .expect("quality text");
    let quality_items: Value = serde_json::from_str(quality_text).expect("quality json");
    assert_eq!(quality_items.as_array().expect("quality array").len(), 1);
    assert_eq!(quality_items[0]["id"], "mem-quality-workflow");
    assert_eq!(quality_items[0]["kind"], "workflow");
    assert_eq!(quality_items[0]["quality"]["score"], 75);
    assert_eq!(
        quality_items[0]["quality"]["findings"],
        json!(["content is too short to be reusable"])
    );

    child.kill().expect("kill server");
    let _ = child.wait();
}

#[test]
fn mcp_serve_exposes_stale_memory_review_queue() {
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
                    "id": "mem-stale-original",
                    "kind": "workflow",
                    "content": "Run pnpm verify before pushing main.",
                    "session_id": "session-stale-older",
                    "turn_id": "turn-stale-older"
                }
            }
        }),
    );
    let created_original = read_response(&mut stdout);
    assert_eq!(created_original["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_approve",
                "arguments": {
                    "id": "mem-stale-original",
                    "actor": "mcp-client"
                }
            }
        }),
    );
    let approved_original = read_response(&mut stdout);
    assert_eq!(approved_original["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-stale-newer",
                    "kind": "workflow",
                    "content": "Run pnpm verify before pushing to main branch.",
                    "session_id": "session-stale-newer",
                    "turn_id": "turn-stale-newer"
                }
            }
        }),
    );
    let created_newer = read_response(&mut stdout);
    assert_eq!(created_newer["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "memory_stale",
                "arguments": {
                    "kind": "workflow",
                    "threshold": 70
                }
            }
        }),
    );
    let stale = read_response(&mut stdout);
    assert_eq!(stale["result"]["isError"], false);
    let stale_text = stale["result"]["content"][0]["text"]
        .as_str()
        .expect("stale text");
    let stale_items: Value = serde_json::from_str(stale_text).expect("stale json");
    assert_eq!(stale_items[0]["id"], "mem-stale-original");
    assert_eq!(stale_items[0]["status"], "approved");
    assert_eq!(stale_items[0]["kind"], "workflow");
    assert_eq!(stale_items[0]["score"], 75);
    assert_eq!(stale_items[0]["newer"]["id"], "mem-stale-newer");
    assert_eq!(stale_items[0]["newer"]["status"], "candidate");
    assert_eq!(
        stale_items[0]["newer"]["source_uri"],
        "codel00p://sessions/session-stale-newer"
    );

    child.kill().expect("kill server");
    let _ = child.wait();
}

#[test]
fn mcp_serve_exposes_sensitive_memory_search_filter() {
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
                    "id": "mem-mcp-sensitive",
                    "kind": "workflow",
                    "content": "Use the private verify credential only from CI.",
                    "session_id": "session-sensitive",
                    "turn_id": "turn-sensitive",
                    "sensitivity": "sensitive"
                }
            }
        }),
    );
    let created = read_response(&mut stdout);
    assert_eq!(created["result"]["isError"], false);
    let created_text = created["result"]["content"][0]["text"]
        .as_str()
        .expect("created text");
    assert!(created_text.contains(r#""sensitivity":"sensitive""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_approve",
                "arguments": {
                    "id": "mem-mcp-sensitive",
                    "actor": "mcp-client"
                }
            }
        }),
    );
    let approved = read_response(&mut stdout);
    assert_eq!(approved["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "memory_search",
                "arguments": {
                    "text": "verify",
                    "sensitivity": "sensitive"
                }
            }
        }),
    );
    let searched = read_response(&mut stdout);
    assert_eq!(searched["result"]["isError"], false);
    let searched_text = searched["result"]["content"][0]["text"]
        .as_str()
        .expect("searched text");
    let records: Value = serde_json::from_str(searched_text).expect("searched json");
    assert_eq!(records[0]["id"], "mem-mcp-sensitive");
    assert_eq!(records[0]["sensitivity"], "sensitive");
    assert_eq!(
        records[0]["reason"],
        "matched text verify and sensitivity sensitive"
    );

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
    assert!(
        memory_text
            .contains(r#""source":{"session_id":"session-resource","turn_id":"turn-resource"}"#)
    );
    assert!(memory_text.contains(r#""source_uri":"codel00p://sessions/session-resource""#));

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
fn mcp_serve_emits_progress_notifications_for_tokenized_requests() {
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
                "_meta": {
                    "progressToken": "progress-memory-list"
                },
                "name": "memory_list",
                "arguments": { "limit": 5 }
            }
        }),
    );
    let started = read_message(&mut stdout);
    assert_eq!(started["method"], "notifications/progress");
    assert_eq!(started["params"]["progressToken"], "progress-memory-list");
    assert_eq!(started["params"]["progress"], 1);
    assert_eq!(started["params"]["total"], 2);

    let completed = read_message(&mut stdout);
    assert_eq!(completed["method"], "notifications/progress");
    assert_eq!(completed["params"]["progressToken"], "progress-memory-list");
    assert_eq!(completed["params"]["progress"], 2);
    assert_eq!(completed["params"]["total"], 2);

    let response = read_response(&mut stdout);
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["content"][0]["text"], "[]");

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/read",
            "params": {
                "_meta": {
                    "progressToken": 42
                },
                "uri": "codel00p://sessions/session-progress"
            }
        }),
    );
    let resource_started = read_message(&mut stdout);
    assert_eq!(resource_started["method"], "notifications/progress");
    assert_eq!(resource_started["params"]["progressToken"], 42);
    assert_eq!(resource_started["params"]["progress"], 1);

    let resource_completed = read_message(&mut stdout);
    assert_eq!(resource_completed["method"], "notifications/progress");
    assert_eq!(resource_completed["params"]["progressToken"], 42);
    assert_eq!(resource_completed["params"]["progress"], 2);

    let resource_response = read_response(&mut stdout);
    assert_eq!(resource_response["id"], 3);
    assert_eq!(resource_response["result"]["contents"][0]["text"], "[]");

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
                    "kind": "workflow",
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
                "name": "memory_edit",
                "arguments": {
                    "id": "mem-mcp-1",
                    "actor": "mcp-client",
                    "content": "Use MCP edit tools for reviewed project memory.",
                    "reason": "clarified editable workflow"
                }
            }
        }),
    );
    let edited = read_response(&mut stdout);
    assert_eq!(edited["result"]["isError"], false);
    let edited_text = edited["result"]["content"][0]["text"]
        .as_str()
        .expect("edited text");
    assert!(edited_text.contains(r#""status":"approved""#));
    assert!(edited_text.contains("Use MCP edit tools for reviewed project memory."));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "memory_audit",
                "arguments": {
                    "id": "mem-mcp-1"
                }
            }
        }),
    );
    let audit = read_response(&mut stdout);
    assert_eq!(audit["result"]["isError"], false);
    let audit_text = audit["result"]["content"][0]["text"]
        .as_str()
        .expect("audit text");
    assert!(audit_text.contains(r#""memory_id":"mem-mcp-1""#));
    assert!(audit_text.contains(r#""sequence":3"#));
    assert!(audit_text.contains(r#""action":"edited""#));
    assert!(audit_text.contains(r#""actor":"mcp-client""#));
    assert!(audit_text.contains(r#""reason":"clarified editable workflow""#));
    assert!(
        audit_text
            .contains(r#""previous_content":"Use MCP write tools for reviewed project memory.""#)
    );
    assert!(
        audit_text.contains(r#""new_content":"Use MCP edit tools for reviewed project memory.""#)
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "memory_search",
                "arguments": { "text": "MCP edit tools" }
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
    assert!(
        searched_text.contains(r#""source":{"session_id":"session-mcp","turn_id":"turn-mcp"}"#)
    );
    assert!(searched_text.contains(r#""source_uri":"codel00p://sessions/session-mcp""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "memory_list",
                "arguments": { "status": "approved", "limit": 1 }
            }
        }),
    );
    let listed = read_response(&mut stdout);
    assert_eq!(listed["result"]["isError"], false);
    let listed_text = listed["result"]["content"][0]["text"]
        .as_str()
        .expect("listed text");
    assert!(listed_text.contains(r#""id":"mem-mcp-1""#));
    assert!(listed_text.contains(r#""source":{"session_id":"session-mcp","turn_id":"turn-mcp"}"#));
    assert!(listed_text.contains(r#""source_uri":"codel00p://sessions/session-mcp""#));
    let listed_items: Value = serde_json::from_str(listed_text).expect("listed json");
    assert_eq!(listed_items[0]["quality"]["score"], 100);
    assert_eq!(listed_items[0]["quality"]["findings"], json!([]));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "memory_similar",
                "arguments": {
                    "content": "Use MCP edit tools for reviewed project memory updates.",
                    "kind": "workflow",
                    "threshold": 70
                }
            }
        }),
    );
    let similar = read_response(&mut stdout);
    assert_eq!(similar["result"]["isError"], false);
    let similar_text = similar["result"]["content"][0]["text"]
        .as_str()
        .expect("similar text");
    assert!(similar_text.contains(r#""id":"mem-mcp-1""#));
    assert!(similar_text.contains(r#""score":89"#));
    assert!(similar_text.contains(r#""source":{"session_id":"session-mcp","turn_id":"turn-mcp"}"#));
    assert!(similar_text.contains(r#""source_uri":"codel00p://sessions/session-mcp""#));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "memory_restore",
                "arguments": {
                    "id": "mem-mcp-1",
                    "sequence": 3,
                    "actor": "mcp-client",
                    "reason": "undo edit"
                }
            }
        }),
    );
    let restored = read_response(&mut stdout);
    assert_eq!(restored["result"]["isError"], false);
    let restored_text = restored["result"]["content"][0]["text"]
        .as_str()
        .expect("restored text");
    assert!(restored_text.contains(r#""status":"approved""#));
    assert!(restored_text.contains("Use MCP write tools for reviewed project memory."));

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "memory_audit",
                "arguments": {
                    "id": "mem-mcp-1"
                }
            }
        }),
    );
    let restored_audit = read_response(&mut stdout);
    assert_eq!(restored_audit["result"]["isError"], false);
    let restored_audit_text = restored_audit["result"]["content"][0]["text"]
        .as_str()
        .expect("restored audit text");
    assert!(restored_audit_text.contains(r#""memory_id":"mem-mcp-1""#));
    assert!(restored_audit_text.contains(r#""sequence":4"#));
    assert!(restored_audit_text.contains(r#""action":"edited""#));
    assert!(restored_audit_text.contains(r#""reason":"undo edit""#));
    assert!(
        restored_audit_text
            .contains(r#""previous_content":"Use MCP edit tools for reviewed project memory.""#)
    );
    assert!(
        restored_audit_text
            .contains(r#""new_content":"Use MCP write tools for reviewed project memory.""#)
    );

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 11,
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
            "id": 12,
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
            "id": 13,
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
