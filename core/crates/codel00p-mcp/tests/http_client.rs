use codel00p_mcp::{HttpServerEndpoint, McpHttpClient, McpToolCall};
use httpmock::{Method::POST, MockServer};
use serde_json::json;

#[tokio::test]
async fn http_client_initializes_and_reuses_session_header() {
    let server = MockServer::start();
    let initialize = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"initialize""#);
        then.status(200)
            .header("Mcp-Session-Id", "session-1")
            .json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "remote", "version": "1.0.0" }
                }
            }));
    });
    let initialized = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .header("mcp-session-id", "session-1")
            .body_includes(r#""method":"notifications/initialized""#);
        then.status(202);
    });
    let list = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .header("mcp-session-id", "session-1")
            .body_includes(r#""method":"tools/list""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "search",
                        "description": "Search remote docs.",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        }));
    });

    let endpoint = HttpServerEndpoint::new("remote", format!("{}/mcp", server.base_url()));
    let mut client = McpHttpClient::connect(endpoint).expect("connect http client");

    let initialization = client.initialize().await.expect("initialize");
    assert_eq!(initialization.server_name(), Some("remote"));
    let tools = client.list_tools().await.expect("list tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].harness_tool_name(), "mcp.remote.search");

    initialize.assert();
    initialized.assert();
    list.assert();
}

#[tokio::test]
async fn http_client_calls_tools_over_json_rpc_post() {
    let server = MockServer::start();
    let call = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .header("authorization", "Bearer test-token")
            .body_includes(r#""method":"tools/call""#)
            .body_includes(r#""name":"lookup""#)
            .body_includes(r#""query":"memory""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "content": [
                    { "type": "text", "text": "found" }
                ],
                "isError": false
            }
        }));
    });

    let endpoint = HttpServerEndpoint::new("remote", format!("{}/mcp", server.base_url()))
        .with_bearer_token("test-token");
    let mut client = McpHttpClient::connect(endpoint).expect("connect http client");

    let output = client
        .call_tool(McpToolCall::new(
            "remote",
            "lookup",
            json!({ "query": "memory" }),
        ))
        .await
        .expect("call tool");
    assert_eq!(output.content()["content"][0]["text"], "found");
    call.assert();
}

#[tokio::test]
async fn http_client_accepts_sse_json_rpc_responses() {
    let server = MockServer::start();
    let list = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"tools/list""#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(
                "event: message\n\
                 data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n",
            );
    });

    let endpoint = HttpServerEndpoint::new("remote", format!("{}/mcp", server.base_url()));
    let mut client = McpHttpClient::connect(endpoint).expect("connect http client");

    let tools = client.list_tools().await.expect("list tools");
    assert!(tools.is_empty());
    list.assert();
}
