use super::support::*;

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
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-quality-sensitive",
                    "kind": "workflow",
                    "content": "Use credential.",
                    "sensitivity": "sensitive",
                    "tags": ["credential"],
                    "session_id": "session-quality",
                    "turn_id": "turn-quality"
                }
            }
        }),
    );
    let sensitive = read_response(&mut stdout);
    assert_eq!(sensitive["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-quality-sensitive-other",
                    "kind": "workflow",
                    "content": "Use secret.",
                    "sensitivity": "sensitive",
                    "tags": ["other"],
                    "session_id": "session-quality",
                    "turn_id": "turn-quality"
                }
            }
        }),
    );
    let sensitive_other = read_response(&mut stdout);
    assert_eq!(sensitive_other["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "memory_create_candidate",
                "arguments": {
                    "id": "mem-quality-sensitive-candidate",
                    "kind": "workflow",
                    "content": "Use token.",
                    "sensitivity": "sensitive",
                    "tags": ["credential"],
                    "session_id": "session-quality",
                    "turn_id": "turn-quality"
                }
            }
        }),
    );
    let sensitive_candidate = read_response(&mut stdout);
    assert_eq!(sensitive_candidate["result"]["isError"], false);

    send(
        &mut child,
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "memory_approve",
                "arguments": {
                    "id": "mem-quality-sensitive",
                    "actor": "alice"
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
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "memory_quality",
                "arguments": {
                    "status": "approved",
                    "kind": "workflow",
                    "sensitivity": "sensitive",
                    "tag": "credential",
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
    assert_eq!(quality_items[0]["id"], "mem-quality-sensitive");
    assert_eq!(quality_items[0]["status"], "approved");
    assert_eq!(quality_items[0]["kind"], "workflow");
    assert_eq!(quality_items[0]["sensitivity"], "sensitive");
    assert_eq!(quality_items[0]["tags"], json!(["credential"]));
    assert_eq!(quality_items[0]["quality"]["score"], 75);
    assert_eq!(
        quality_items[0]["quality"]["findings"],
        json!(["content is too short to be reusable"])
    );

    child.kill().expect("kill server");
    let _ = child.wait();
}
