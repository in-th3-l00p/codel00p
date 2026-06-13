use super::support::*;

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
