use super::support::*;

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
    assert!(tool_names.contains(&"memory_merge"));
    assert!(tool_names.contains(&"memory_split"));
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
fn mcp_memory_merge_archives_source_and_returns_target() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let mut child = spawn_codel00p_mcp_server(&db_path);
    let stdout = child.stdout.take().expect("stdout");
    let mut stdout = BufReader::new(stdout);

    send(
        &mut child,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    read_response(&mut stdout);
    send(
        &mut child,
        json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
    );

    // Seed two candidates via MCP.
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mcp-dup",
                    "kind": "convention",
                    "content": "Run cargo from core.",
                    "session_id": "s1",
                    "turn_id": "t1"
                }
            }
        }),
    );
    read_response(&mut stdout);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0", "id": 3,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mcp-keep",
                    "kind": "convention",
                    "content": "Run cargo commands from core/.",
                    "session_id": "s1",
                    "turn_id": "t2"
                }
            }
        }),
    );
    read_response(&mut stdout);

    // Approve both.
    send(
        &mut child,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory_approve","arguments":{"id":"mcp-dup","actor":"alice"}}}),
    );
    read_response(&mut stdout);
    send(
        &mut child,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"memory_approve","arguments":{"id":"mcp-keep","actor":"alice"}}}),
    );
    read_response(&mut stdout);

    // Merge dup into keep.
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "memory_merge",
                "arguments": {
                    "source_id": "mcp-dup",
                    "target_id": "mcp-keep",
                    "actor": "alice",
                    "reason": "near-duplicate"
                }
            }
        }),
    );
    let merge_resp = read_response(&mut stdout);
    assert_eq!(merge_resp["result"]["isError"], false);
    let text = merge_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let record: serde_json::Value = serde_json::from_str(text).expect("json");
    assert_eq!(record["id"], "mcp-keep");
    assert_eq!(record["status"], "approved");

    child.kill().expect("kill server");
    let _ = child.wait();
}

#[test]
fn mcp_memory_split_creates_candidate_from_source() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let mut child = spawn_codel00p_mcp_server(&db_path);
    let stdout = child.stdout.take().expect("stdout");
    let mut stdout = BufReader::new(stdout);

    send(
        &mut child,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    read_response(&mut stdout);
    send(
        &mut child,
        json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
    );

    // Seed and approve a source memory.
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mcp-src",
                    "kind": "convention",
                    "content": "Use cargo from core/. Run tests serially.",
                    "session_id": "s1",
                    "turn_id": "t1",
                    "tags": ["cargo"]
                }
            }
        }),
    );
    read_response(&mut stdout);
    send(
        &mut child,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_approve","arguments":{"id":"mcp-src","actor":"alice"}}}),
    );
    read_response(&mut stdout);

    // Split the source memory.
    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "memory_split",
                "arguments": {
                    "source_id": "mcp-src",
                    "new_id": "mcp-split-new",
                    "actor": "alice",
                    "content": "Run tests serially.",
                    "reason": "split testing note"
                }
            }
        }),
    );
    let split_resp = read_response(&mut stdout);
    assert_eq!(split_resp["result"]["isError"], false);
    let text = split_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let record: serde_json::Value = serde_json::from_str(text).expect("json");
    assert_eq!(record["id"], "mcp-split-new");
    assert_eq!(record["status"], "candidate");
    assert_eq!(record["content"], "Run tests serially.");
    // New memory inherits tags from source.
    assert_eq!(record["tags"], json!(["cargo"]));

    child.kill().expect("kill server");
    let _ = child.wait();
}
