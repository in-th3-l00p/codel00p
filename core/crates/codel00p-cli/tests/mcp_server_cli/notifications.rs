use super::support::*;

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
