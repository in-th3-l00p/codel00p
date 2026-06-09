use std::{
    fs,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codel00p_mcp::{
    McpClientNotification, McpNotificationWorker, McpPromptMessage, McpReconnectPolicy,
    McpStdioClient, McpStdioNotificationSupervisor, McpSubscriptionEvent, McpToolCall,
    StdioServerCommand,
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
async fn stdio_client_lists_and_gets_mcp_prompts() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read list; case "$list" in *'"method":"prompts/list"'*) ;; *) exit 11;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"prompts":[{"name":"review_pr","description":"Review a pull request.","arguments":[{"name":"diff","description":"Unified diff","required":true}]}]}}'; read get; case "$get" in *'"method":"prompts/get"'*) ;; *) exit 12;; esac; case "$get" in *'"name":"review_pr"'*) ;; *) exit 13;; esac; case "$get" in *'"diff":"patch"'*) ;; *) exit 14;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"description":"Review a pull request.","messages":[{"role":"user","content":{"type":"text","text":"Review this patch."}}]}}'"#,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let prompts = client.list_prompts().await.expect("list prompts");
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].server_id(), "fake");
    assert_eq!(prompts[0].name(), "review_pr");
    assert_eq!(prompts[0].description(), Some("Review a pull request."));
    assert_eq!(prompts[0].arguments()[0].name(), "diff");
    assert!(prompts[0].arguments()[0].required());

    let prompt = client
        .get_prompt("review_pr", json!({ "diff": "patch" }))
        .await
        .expect("get prompt");
    assert_eq!(prompt.description(), Some("Review a pull request."));
    assert_eq!(
        prompt.messages(),
        &[McpPromptMessage::text("user", "Review this patch.")]
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
async fn stdio_client_maps_logging_and_prompt_change_notifications() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read line; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/message","params":{"level":"warning","logger":"fake","data":{"message":"check auth"}}}'; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/prompts/list_changed","params":{}}'; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}],"isError":false}}'"#,
        ],
    );
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let output = client
        .call_tool(McpToolCall::new("fake", "echo", json!({})))
        .await
        .expect("call tool");

    assert_eq!(
        output.notifications(),
        &[
            McpClientNotification::log_message(
                "warning",
                Some("fake"),
                json!({ "message": "check auth" })
            ),
            McpClientNotification::prompts_list_changed(),
        ]
    );
}

#[tokio::test]
async fn stdio_client_answers_roots_list_requests_during_tool_discovery() {
    let command = StdioServerCommand::new(
        "fake",
        "/bin/sh",
        [
            "-c",
            r#"read list; printf '%s\n' '{"jsonrpc":"2.0","id":"roots-1","method":"roots/list","params":{}}'; read roots; case "$roots" in *'"id":"roots-1"'*) ;; *) exit 21;; esac; case "$roots" in *'"roots"'*) ;; *) exit 22;; esac; case "$roots" in *'"file:///workspace"'*) ;; *) exit 23;; esac; case "$roots" in *'"workspace"'*) ;; *) exit 24;; esac; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}'"#,
        ],
    )
    .with_root("file:///workspace", Some("workspace"));
    let mut client = McpStdioClient::spawn(command)
        .await
        .expect("spawn stdio client");

    let tools = client.list_tools().await.expect("list tools");

    assert!(tools.is_empty());
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
async fn notification_supervisor_reconnects_stdio_servers_and_resubscribes() {
    let dir = std::env::temp_dir().join(format!(
        "codel00p-mcp-reconnect-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    let attempt_path = dir.join("attempt");
    let script = dir.join("server.sh");
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
attempt_file={}
attempt=0
if [ -f "$attempt_file" ]; then
  attempt=$(cat "$attempt_file")
fi
attempt=$((attempt + 1))
printf '%s' "$attempt" > "$attempt_file"
read init
printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"2025-06-18","capabilities":{{"resources":{{"subscribe":true}}}},"serverInfo":{{"name":"fake","version":"1"}}}}}}'
read initialized
read subscribe
printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{}}}}'
if [ "$attempt" = "1" ]; then
  printf '%s\n' '{{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{{"uri":"codel00p://memory/first"}}}}'
  exit 0
fi
printf '%s\n' '{{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{{"uri":"codel00p://memory/second"}}}}'
sleep 1
"#,
            attempt_path.display()
        ),
    )
    .expect("write fake server");
    let command = StdioServerCommand::new("fake", "/bin/sh", [script.to_string_lossy().as_ref()])
        .with_request_timeout(Duration::from_millis(500));
    let policy = McpReconnectPolicy::new(2, Duration::from_millis(0));
    let mut supervisor = McpStdioNotificationSupervisor::spawn(
        command,
        ["codel00p://memory/shared".to_string()],
        policy,
    );

    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("connected")
            .expect("connected event"),
        McpSubscriptionEvent::connected("fake", 1)
    );
    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("subscribed")
            .expect("subscribed event"),
        McpSubscriptionEvent::subscribed("codel00p://memory/shared")
    );
    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("first notification")
            .expect("first notification event"),
        McpSubscriptionEvent::notification(McpClientNotification::resource_updated(
            "codel00p://memory/first"
        ))
    );
    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("reconnecting")
            .expect("reconnecting event"),
        McpSubscriptionEvent::reconnecting(
            "fake",
            2,
            "mcp stdio transport failed for fake: server closed stdout before notification"
        )
    );
    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("connected again")
            .expect("connected again event"),
        McpSubscriptionEvent::connected("fake", 2)
    );
    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("resubscribed")
            .expect("resubscribed event"),
        McpSubscriptionEvent::subscribed("codel00p://memory/shared")
    );
    assert_eq!(
        supervisor
            .recv()
            .await
            .expect("second notification")
            .expect("second notification event"),
        McpSubscriptionEvent::notification(McpClientNotification::resource_updated(
            "codel00p://memory/second"
        ))
    );
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
