use super::support::*;

#[test]
fn agent_run_calls_configured_http_mcp_tools() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::create_dir(workspace.join(".codel00p")).expect("create config dir");

    let mcp = MockServer::start();
    let initialize = mcp.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"initialize""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "2025-06-18",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "docs", "version": "1.0.0" }
            }
        }));
    });
    let initialized = mcp.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"notifications/initialized""#);
        then.status(202);
    });
    let list = mcp.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"tools/list""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "lookup",
                        "description": "Lookup remote docs.",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        }));
    });
    let call = mcp.mock(|when, then| {
        when.method(POST)
            .path("/mcp")
            .body_includes(r#""method":"tools/call""#)
            .body_includes(r#""name":"lookup""#)
            .body_includes(r#""query":"memory""#);
        then.status(200).json_body(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "content": [
                    { "type": "text", "text": "remote memory docs" }
                ],
                "isError": false
            }
        }));
    });
    fs::write(
        workspace.join(".codel00p/mcp.json"),
        json!({
            "servers": {
                "docs": {
                    "url": format!("{}/mcp", mcp.base_url())
                }
            }
        })
        .to_string(),
    )
    .expect("write mcp config");

    let provider = MockServer::start();
    let first = provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"mcp.docs.lookup""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-lookup",
                                "type": "function",
                                "function": {
                                    "name": "mcp.docs.lookup",
                                    "arguments": "{\"query\":\"memory\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ]
        }));
    });
    let second = provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("remote memory docs");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "HTTP MCP worked."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Search remote docs.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &provider.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "HTTP MCP worked.\n");
    initialize.assert();
    initialized.assert();
    list.assert();
    call.assert();
    first.assert();
    second.assert();
}

#[test]
fn agent_run_denies_mcp_tools_as_external_connectors_in_json_events() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    let mcp_server = dir.path().join("fake-denied-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"send","description":"Send externally.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}'
read maybe_call
exit 2
"#,
    )
    .expect("write fake mcp server");
    let chmod = Command::new("chmod")
        .arg("+x")
        .arg(&mcp_server)
        .output()
        .expect("chmod fake server");
    assert!(chmod.status.success(), "stderr: {}", stderr(&chmod));

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"mcp.fake.send""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-send",
                                "type": "function",
                                "function": {
                                    "name": "mcp.fake.send",
                                    "arguments": "{\"text\":\"hello\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ]
        }));
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("permission_denied")
            .body_includes("mcp.fake.send denied by CLI permission mode");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "MCP denied."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Use the MCP send tool.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--mcp-server",
            &format!("fake={}", mcp_server.display()),
            "--permission-mode",
            "deny",
            "--json-events",
            "--session-id",
            "session-mcp-denied",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let run_stdout = stdout(&output);
    assert!(
        run_stdout.starts_with("MCP denied.\n"),
        "stdout: {run_stdout}"
    );
    assert!(run_stdout.contains(r#""kind":"permission_requested""#));
    assert!(run_stdout.contains(r#""tool_name":"mcp.fake.send""#));
    assert!(run_stdout.contains(r#""scope":"external_connector""#));
    assert!(run_stdout.contains(r#""kind":"permission_denied""#));

    let show = run_codel00p(&db_path, &["session", "show", "session-mcp-denied"]);
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    let session_output = stdout(&show);
    assert!(session_output.contains("\tevent\tpermission_requested\t\n"));
    assert!(session_output.contains("\tevent\tpermission_denied\t\n"));
    first.assert();
    second.assert();
}
