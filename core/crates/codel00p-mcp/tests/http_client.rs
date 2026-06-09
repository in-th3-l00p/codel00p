use codel00p_mcp::{HttpServerEndpoint, McpClientNotification, McpHttpClient, McpToolCall};
use httpmock::{HttpMockRequest, Method::POST, MockServer};
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
async fn http_client_collects_sse_notifications_before_tool_response() {
    let server = MockServer::start();
    let call = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"tools/call""#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(
                "event: message\n\
                 data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{\"progressToken\":\"p1\",\"progress\":1,\"total\":2,\"message\":\"Working\"}}\n\n\
                 event: message\n\
                 data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/resources/updated\",\"params\":{\"uri\":\"codel00p://memory/mem-1\"}}\n\n\
                 event: message\n\
                 data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"done\"}],\"isError\":false}}\n\n",
            );
    });

    let endpoint = HttpServerEndpoint::new("remote", format!("{}/mcp", server.base_url()));
    let mut client = McpHttpClient::connect(endpoint).expect("connect http client");

    let output = client
        .call_tool(McpToolCall::new("remote", "lookup", json!({})))
        .await
        .expect("call tool");

    assert_eq!(output.content()["content"][0]["text"], "done");
    assert_eq!(
        output.notifications(),
        &[
            McpClientNotification::progress(json!("p1"), 1.0, Some(2.0), Some("Working")),
            McpClientNotification::resource_updated("codel00p://memory/mem-1")
        ]
    );
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

#[tokio::test]
async fn http_client_paginates_tool_lists_until_cursor_is_absent() {
    let server = MockServer::start();
    let first_page = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"tools/list""#)
            .is_true(|request: &HttpMockRequest| !request.body_string().contains(r#""cursor""#));
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "tools": [
                    {
                        "name": "search",
                        "description": "Search remote docs.",
                        "inputSchema": { "type": "object" }
                    }
                ],
                "nextCursor": "tools-2"
            }
        }));
    });
    let second_page = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"tools/list""#)
            .body_includes(r#""cursor":"tools-2""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "open",
                        "description": "Open remote docs.",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        }));
    });

    let endpoint = HttpServerEndpoint::new("remote", format!("{}/mcp", server.base_url()));
    let mut client = McpHttpClient::connect(endpoint).expect("connect http client");

    let tools = client.list_tools().await.expect("list tools");

    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].harness_tool_name(), "mcp.remote.search");
    assert_eq!(tools[1].harness_tool_name(), "mcp.remote.open");
    first_page.assert();
    second_page.assert();
}

#[tokio::test]
async fn http_client_supports_prompts_resources_and_logging() {
    let server = MockServer::start();
    let prompts = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"prompts/list""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "prompts": [
                    {
                        "name": "review",
                        "description": "Review code.",
                        "arguments": [
                            { "name": "diff", "required": true }
                        ]
                    }
                ]
            }
        }));
    });
    let get_prompt = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"prompts/get""#)
            .body_includes(r#""name":"review""#)
            .body_includes(r#""diff":"patch""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "messages": [
                    {
                        "role": "user",
                        "content": { "type": "text", "text": "Review patch." }
                    }
                ]
            }
        }));
    });
    let templates = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"resources/templates/list""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "resourceTemplates": [
                    {
                        "uriTemplate": "file:///{path}",
                        "name": "workspace file",
                        "mimeType": "text/plain"
                    }
                ]
            }
        }));
    });
    let read = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"resources/read""#)
            .body_includes(r#""uri":"file:///README.md""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 4,
            "result": {
                "contents": [
                    {
                        "uri": "file:///README.md",
                        "mimeType": "text/markdown",
                        "text": "# codel00p"
                    }
                ]
            }
        }));
    });
    let logging = server.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"logging/setLevel""#)
            .body_includes(r#""level":"warning""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 5,
            "result": {}
        }));
    });

    let endpoint = HttpServerEndpoint::new("remote", format!("{}/mcp", server.base_url()));
    let mut client = McpHttpClient::connect(endpoint).expect("connect http client");

    let prompt_descriptors = client.list_prompts().await.expect("list prompts");
    assert_eq!(prompt_descriptors[0].name(), "review");
    assert!(prompt_descriptors[0].arguments()[0].required());
    let prompt = client
        .get_prompt("review", json!({ "diff": "patch" }))
        .await
        .expect("get prompt");
    assert_eq!(prompt.messages()[0].role(), "user");
    assert_eq!(prompt.messages()[0].content()["text"], "Review patch.");
    let resource_templates = client
        .list_resource_templates()
        .await
        .expect("list resource templates");
    assert_eq!(resource_templates[0].uri_template(), "file:///{path}");
    let resource = client
        .read_resource("file:///README.md")
        .await
        .expect("read resource");
    assert_eq!(resource.contents()[0].text(), Some("# codel00p"));
    client
        .set_logging_level("warning")
        .await
        .expect("set logging level");

    prompts.assert();
    get_prompt.assert();
    templates.assert();
    read.assert();
    logging.assert();
}
