use super::support::*;

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
