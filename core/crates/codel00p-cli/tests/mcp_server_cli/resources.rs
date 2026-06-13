use super::support::*;

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
                    "source_uri": "https://github.com/in-th3-l00p/codel00p/pull/1",
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
    let memory_json: Value = serde_json::from_str(memory_text).expect("memory resource json");
    assert!(memory_text.contains(r#""id":"mem-resource-1""#));
    assert!(memory_text.contains(r#""kind":"architecture""#));
    assert_eq!(memory_json["source"]["session_id"], "session-resource");
    assert_eq!(memory_json["source"]["turn_id"], "turn-resource");
    assert_eq!(
        memory_json["source"]["uri"],
        "https://github.com/in-th3-l00p/codel00p/pull/1"
    );
    assert_eq!(
        memory_json["source_uri"],
        "https://github.com/in-th3-l00p/codel00p/pull/1"
    );

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
