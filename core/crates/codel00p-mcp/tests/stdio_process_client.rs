use std::{
    fs,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codel00p_mcp::{
    McpClientNotification, McpNotificationWorker, McpStdioClient, McpToolCall, StdioServerCommand,
};
use serde_json::json;
use tokio::sync::Mutex;

#[tokio::test]
async fn stdio_client_sends_json_rpc_requests_to_process_and_reads_responses() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            "read line; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo input.\",\"inputSchema\":{\"type\":\"object\"}}]}}'",
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let response = client
        .request("tools/list", json!({}))
        .await
        .expect("tools/list response");

    assert_eq!(
        response,
        json!({
            "tools": [
                {
                    "name": "echo",
                    "description": "Echo input.",
                    "inputSchema": { "type": "object" }
                }
            ]
        })
    );
}

#[tokio::test]
async fn stdio_client_lists_and_calls_mcp_tools() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            "read first; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"Echo input.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"}}}}]}}'; read second; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"hello\"}],\"isError\":false}}'",
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let tools = client.list_tools().await.expect("list tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].server_id(), "fake");
    assert_eq!(tools[0].tool_name(), "echo");
    assert_eq!(tools[0].description(), "Echo input.");
    assert_eq!(
        tools[0].input_schema()["properties"]["text"]["type"],
        "string"
    );

    let output = client
        .call_tool(McpToolCall::new("fake", "echo", json!({ "text": "hello" })))
        .await
        .expect("call tool");

    assert_eq!(
        output.content(),
        &json!({
            "content": [
                { "type": "text", "text": "hello" }
            ],
            "isError": false
        })
    );
}

#[tokio::test]
async fn stdio_client_collects_notifications_before_tool_response() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read line; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"p1","progress":1,"total":2,"message":"Searching memory"}}'; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"uri":"codel00p://memory/mem-1"}}'; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}],"isError":false}}'"#,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let output = client
        .call_tool(McpToolCall::new("fake", "echo", json!({})))
        .await
        .expect("call tool");

    assert_eq!(output.content()["content"][0]["text"], "ok");
    assert_eq!(
        output.notifications(),
        &[
            McpClientNotification::progress(json!("p1"), 1.0, Some(2.0), Some("Searching memory")),
            McpClientNotification::resource_updated("codel00p://memory/mem-1")
        ]
    );
}

#[tokio::test]
async fn stdio_client_reads_notifications_after_resource_subscription() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read subscribe; case "$subscribe" in *'"method":"resources/subscribe"'*) ;; *) exit 11;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{}}'; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"uri":"codel00p://memory/mem-1"}}'; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/tools/list_changed","params":{}}'; read unsubscribe; case "$unsubscribe" in *'"method":"resources/unsubscribe"'*) ;; *) exit 12;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{}}'"#,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    client
        .subscribe_resource("codel00p://memory/mem-1")
        .await
        .expect("subscribe resource");
    assert_eq!(
        client.read_notification().await.expect("resource update"),
        McpClientNotification::resource_updated("codel00p://memory/mem-1")
    );
    assert_eq!(
        client
            .read_notification()
            .await
            .expect("tools list changed"),
        McpClientNotification::tools_list_changed()
    );
    client
        .unsubscribe_resource("codel00p://memory/mem-1")
        .await
        .expect("unsubscribe resource");
}

#[tokio::test]
async fn notification_worker_subscribes_and_streams_stdio_notifications() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read subscribe; case "$subscribe" in *'"method":"resources/subscribe"'*) ;; *) exit 11;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{}}'; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"uri":"codel00p://memory/mem-2"}}'; sleep 1"#,
        ],
    )
    .with_request_timeout(Duration::from_millis(500));
    let client = Arc::new(Mutex::new(
        McpStdioClient::spawn(command)
            .await
            .expect("spawn stdio client"),
    ));
    let mut worker =
        McpNotificationWorker::spawn_stdio(client, ["codel00p://memory/mem-2".to_string()]);

    assert_eq!(
        worker
            .recv()
            .await
            .expect("worker should emit")
            .expect("notification should succeed"),
        McpClientNotification::resource_updated("codel00p://memory/mem-2")
    );
}

#[tokio::test]
async fn notification_worker_reports_subscription_failures() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read subscribe; printf '%s\n' '{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"subscription denied"}}'"#,
        ],
    );
    let client = Arc::new(Mutex::new(
        McpStdioClient::spawn(command)
            .await
            .expect("spawn stdio client"),
    ));
    let mut worker =
        McpNotificationWorker::spawn_stdio(client, ["codel00p://memory/mem-3".to_string()]);

    let error = worker
        .recv()
        .await
        .expect("worker should emit")
        .expect_err("subscription should fail");

    assert!(error.to_string().contains("subscription denied"));
}

#[tokio::test]
async fn stdio_client_reports_json_rpc_error_responses() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            "read line; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32601,\"message\":\"missing method\"}}'",
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let error = client
        .request("missing/method", json!({}))
        .await
        .expect_err("json-rpc error should fail request");

    assert!(error.to_string().contains("json-rpc error response"));
    assert!(error.to_string().contains("missing method"));
}

#[tokio::test]
async fn stdio_client_times_out_when_server_does_not_respond() {
    let command = StdioServerCommand::new(
        "slow",
        "/bin/sh",
        [
            "-c",
            "read line; sleep 2; printf '%s\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'",
        ],
    )
    .with_request_timeout(Duration::from_millis(50));
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let error = client
        .request("tools/list", json!({}))
        .await
        .expect_err("hung server should time out");

    assert!(error.to_string().contains("timed out"));
    assert!(error.to_string().contains("slow"));
}

#[tokio::test]
async fn stdio_client_shutdown_closes_stdin_and_waits_for_server_exit() {
    let marker = std::env::temp_dir().join(format!(
        "codel00p-mcp-shutdown-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let command = StdioServerCommand::new(
        "shutdown",
        "/bin/sh",
        [
            "-c",
            "while read line; do :; done; printf done > \"$MARKER\"",
        ],
    )
    .with_env("MARKER", marker.as_os_str())
    .with_request_timeout(Duration::from_millis(100));
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    client.shutdown().await.expect("shutdown client");

    assert_eq!(fs::read_to_string(&marker).expect("marker"), "done");
    let _ = fs::remove_file(marker);
}

#[tokio::test]
async fn stdio_client_initializes_before_normal_operations() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read init; case "$init" in *'"method":"initialize"'*) ;; *) exit 11;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{"listChanged":true}},"serverInfo":{"name":"fake-server","version":"1.2.3"},"instructions":"Use fake tools."}}'; read initialized; case "$initialized" in *'"method":"notifications/initialized"'*) ;; *) exit 12;; esac; read list; case "$list" in *'"method":"tools/list"'*) ;; *) exit 13;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}'"#,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let initialized = client.initialize().await.expect("initialize");
    let tools = client.list_tools().await.expect("list tools");

    assert_eq!(initialized.protocol_version(), "2025-06-18");
    assert_eq!(initialized.server_name(), Some("fake-server"));
    assert_eq!(initialized.server_version(), Some("1.2.3"));
    assert_eq!(initialized.instructions(), Some("Use fake tools."));
    assert!(tools.is_empty());
}
