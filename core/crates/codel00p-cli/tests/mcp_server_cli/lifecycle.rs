use super::support::*;

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
