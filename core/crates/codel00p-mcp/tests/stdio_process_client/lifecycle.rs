use super::*;

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
