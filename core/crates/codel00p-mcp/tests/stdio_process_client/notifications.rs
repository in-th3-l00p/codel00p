use super::*;

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
