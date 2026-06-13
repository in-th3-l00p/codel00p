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
