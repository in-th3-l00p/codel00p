use codel00p_mcp::{McpStdioClient, McpToolCall, StdioServerCommand};
use serde_json::json;

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
