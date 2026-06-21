use super::support::*;

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
    // Shingle (token-bigram) Jaccard scores this reworded supersession at 83
    // (was 75 under unigram Jaccard); still above the threshold.
    assert_eq!(stale_items[0]["score"], 83);
    assert_eq!(stale_items[0]["newer"]["id"], "mem-stale-newer");
    assert_eq!(stale_items[0]["newer"]["status"], "candidate");
    assert_eq!(
        stale_items[0]["newer"]["source_uri"],
        "codel00p://sessions/session-stale-newer"
    );

    child.kill().expect("kill server");
    let _ = child.wait();
}
