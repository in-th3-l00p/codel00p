use std::io::Cursor;

use codel00p_mcp::{McpServerHandler, McpServerResponse, McpServerRuntime, serve_stdio_server};
use serde_json::json;

#[test]
fn server_runtime_wraps_dispatch_results_with_progress_and_json_rpc_response() {
    let mut runtime = McpServerRuntime::default();

    let messages = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "_meta": {
                    "progressToken": "token-1"
                },
                "name": "memory_list"
            }
        }),
        |method, params| {
            assert_eq!(method, "tools/call");
            assert_eq!(params["name"], "memory_list");
            Ok(McpServerResponse::new(json!({
                "content": [
                    { "type": "text", "text": "[]" }
                ],
                "isError": false
            })))
        },
    );

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["method"], "notifications/progress");
    assert_eq!(messages[0]["params"]["progressToken"], "token-1");
    assert_eq!(messages[0]["params"]["progress"], 1);
    assert_eq!(messages[0]["params"]["total"], 2);
    assert_eq!(messages[1]["method"], "notifications/progress");
    assert_eq!(messages[1]["params"]["progressToken"], "token-1");
    assert_eq!(messages[1]["params"]["progress"], 2);
    assert_eq!(messages[1]["params"]["total"], 2);
    assert_eq!(messages[2]["id"], 7);
    assert_eq!(messages[2]["result"]["isError"], false);
}

#[test]
fn server_runtime_tracks_resource_subscriptions_and_update_notifications() {
    let mut runtime = McpServerRuntime::default();

    let subscribed = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/subscribe",
            "params": {
                "uri": "codel00p://memory/mem-1"
            }
        }),
        |_method, _params| unreachable!("subscribe is handled by runtime"),
    );
    assert_eq!(
        subscribed,
        vec![json!({ "jsonrpc": "2.0", "id": 1, "result": {} })]
    );

    let updated = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "memory_approve"
            }
        }),
        |_method, _params| {
            Ok(McpServerResponse::new(json!({
                "content": [
                    { "type": "text", "text": "{}" }
                ],
                "isError": false
            }))
            .with_updated_resource("codel00p://memory/mem-1"))
        },
    );
    assert_eq!(updated.len(), 2);
    assert_eq!(updated[0]["id"], 2);
    assert_eq!(updated[1]["method"], "notifications/resources/updated");
    assert_eq!(updated[1]["params"]["uri"], "codel00p://memory/mem-1");

    let unsubscribed = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/unsubscribe",
            "params": {
                "uri": "codel00p://memory/mem-1"
            }
        }),
        |_method, _params| unreachable!("unsubscribe is handled by runtime"),
    );
    assert_eq!(
        unsubscribed,
        vec![json!({ "jsonrpc": "2.0", "id": 3, "result": {} })]
    );

    let updated_after_unsubscribe = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "memory_approve"
            }
        }),
        |_method, _params| {
            Ok(McpServerResponse::new(json!({
                "content": [],
                "isError": false
            }))
            .with_updated_resource("codel00p://memory/mem-1"))
        },
    );
    assert_eq!(updated_after_unsubscribe.len(), 1);
    assert_eq!(updated_after_unsubscribe[0]["id"], 4);
}

#[test]
fn server_runtime_formats_dispatch_errors_and_ignores_notifications() {
    let mut runtime = McpServerRuntime::default();

    let ignored = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
        |_method, _params| unreachable!("notifications have no response"),
    );
    assert!(ignored.is_empty());

    let errored = runtime.handle_request(
        json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/call",
            "params": {
                "_meta": {
                    "progressToken": 99
                },
                "name": "missing"
            }
        }),
        |_method, _params| Err("missing tool".to_string()),
    );
    assert_eq!(errored.len(), 3);
    assert_eq!(errored[0]["method"], "notifications/progress");
    assert_eq!(errored[0]["params"]["progressToken"], 99);
    assert_eq!(errored[2]["id"], "request-1");
    assert_eq!(errored[2]["error"]["code"], -32000);
    assert_eq!(errored[2]["error"]["message"], "missing tool");
}

#[test]
fn server_runtime_can_dispatch_through_a_typed_handler() {
    let mut runtime = McpServerRuntime::default();
    let mut handler = RecordingHandler::default();

    let messages = runtime.handle_request_with_handler(
        json!({
            "jsonrpc": "2.0",
            "id": "typed-1",
            "method": "tools/list",
            "params": {}
        }),
        &mut handler,
    );

    assert_eq!(handler.calls, vec!["tools/list"]);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["id"], "typed-1");
    assert_eq!(messages[0]["result"]["handled"], "tools/list");
}

#[test]
fn stdio_server_runs_a_handler_until_eof_and_writes_newline_delimited_messages() {
    let input = Cursor::new(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":{"progressToken":"p1"},"name":"memory_list"}}
"#,
    );
    let mut output = Vec::new();
    let mut handler = RecordingHandler::default();

    serve_stdio_server(input, &mut output, &mut handler).expect("stdio server should run");

    let output = String::from_utf8(output).expect("stdio output should be utf-8");
    let lines = output.lines().collect::<Vec<_>>();
    assert_eq!(handler.calls, vec!["tools/list", "tools/call"]);
    assert_eq!(lines.len(), 4);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[0]).expect("json")["result"]["handled"],
        "tools/list"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[1]).expect("json")["method"],
        "notifications/progress"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[2]).expect("json")["method"],
        "notifications/progress"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines[3]).expect("json")["result"]["handled"],
        "tools/call"
    );
}

#[derive(Default)]
struct RecordingHandler {
    calls: Vec<String>,
}

impl McpServerHandler for RecordingHandler {
    fn handle_method(
        &mut self,
        method: &str,
        _params: &serde_json::Value,
    ) -> Result<McpServerResponse, String> {
        self.calls.push(method.to_string());
        Ok(McpServerResponse::new(json!({ "handled": method })))
    }
}
