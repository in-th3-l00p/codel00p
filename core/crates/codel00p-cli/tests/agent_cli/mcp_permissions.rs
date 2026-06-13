use super::support::*;

#[test]
fn agent_run_applies_configured_mcp_tool_permission_scopes() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::create_dir(workspace.join(".codel00p")).expect("create config dir");
    let mcp_server = workspace.join("fake-scoped-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search","description":"Search docs.","inputSchema":{"type":"object"}},{"name":"publish","description":"Publish externally.","inputSchema":{"type":"object"}}]}}'
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
    fs::write(
        workspace.join(".codel00p/mcp.json"),
        json!({
            "servers": {
                "docs": {
                    "command": "./fake-scoped-mcp.sh",
                    "permissionScope": "external_connector",
                    "toolScopes": {
                        "search": "read_only"
                    }
                }
            }
        })
        .to_string(),
    )
    .expect("write mcp config");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"mcp.docs.search""#)
            .body_includes(r#""name":"mcp.docs.publish""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-search",
                                "type": "function",
                                "function": {
                                    "name": "mcp.docs.search",
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
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("permission_denied")
            .body_includes("mcp.docs.search denied by CLI permission mode");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Scoped MCP denied."
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
            "Search docs.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--permission-mode",
            "deny",
            "--json-events",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let run_stdout = stdout(&output);
    assert!(run_stdout.starts_with("Scoped MCP denied.\n"));
    assert!(run_stdout.contains(r#""tool_name":"mcp.docs.search""#));
    assert!(run_stdout.contains(r#""scope":"read_only""#));
    assert!(!run_stdout.contains(r#""scope":"external_connector""#));
    first.assert();
    second.assert();
}

#[test]
fn agent_run_remembers_approved_mcp_connector_permissions() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    let mcp_server = dir.path().join("fake-remembered-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"send","description":"Send externally.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}'
read call
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"sent"}],"isError":false}}'
"#,
    )
    .expect("write fake mcp server");
    let chmod = Command::new("chmod")
        .arg("+x")
        .arg(&mcp_server)
        .output()
        .expect("chmod fake server");
    assert!(chmod.status.success(), "stderr: {}", stderr(&chmod));

    let first_provider = MockServer::start();
    let first_initial = first_provider.mock(|when, then| {
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
    let first_final = first_provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("sent")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Approved once."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let first = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "run",
            "Send once.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &first_provider.base_url(),
            "--mcp-server",
            &format!("fake={}", mcp_server.display()),
            "--permission-mode",
            "ask",
            "--remember-permissions",
        ],
        "y\n",
    );

    assert!(first.status.success(), "stderr: {}", stderr(&first));
    assert_eq!(stdout(&first), "Approved once.\n");
    first_initial.assert();
    first_final.assert();

    let second_provider = MockServer::start();
    let second_initial = second_provider.mock(|when, then| {
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
                                "id": "call-send-again",
                                "type": "function",
                                "function": {
                                    "name": "mcp.fake.send",
                                    "arguments": "{\"text\":\"hello again\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ]
        }));
    });
    let second_final = second_provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("sent")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Reused approval."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let second = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Send again.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &second_provider.base_url(),
            "--mcp-server",
            &format!("fake={}", mcp_server.display()),
            "--permission-mode",
            "ask",
            "--remember-permissions",
        ],
    );

    assert!(second.status.success(), "stderr: {}", stderr(&second));
    assert_eq!(stdout(&second), "Reused approval.\n");
    assert!(
        !stderr(&second).contains("Allow tool"),
        "stderr: {}",
        stderr(&second)
    );
    second_initial.assert();
    second_final.assert();
}

#[test]
fn mcp_permissions_can_list_and_forget_remembered_connector_decisions() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    let mcp_server = dir.path().join("fake-permission-list-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"send","description":"Send externally.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}'
read call
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"sent"}],"isError":false}}'
"#,
    )
    .expect("write fake mcp server");
    let chmod = Command::new("chmod")
        .arg("+x")
        .arg(&mcp_server)
        .output()
        .expect("chmod fake server");
    assert!(chmod.status.success(), "stderr: {}", stderr(&chmod));

    let provider = MockServer::start();
    let initial = provider.mock(|when, then| {
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
    let final_response = provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("sent")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Approved once."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let remember = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "run",
            "Send once.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &provider.base_url(),
            "--mcp-server",
            &format!("fake={}", mcp_server.display()),
            "--permission-mode",
            "ask",
            "--remember-permissions",
        ],
        "y\n",
    );

    assert!(remember.status.success(), "stderr: {}", stderr(&remember));
    assert_eq!(stdout(&remember), "Approved once.\n");
    initial.assert();
    final_response.assert();

    let listed = run_codel00p(&db_path, &["mcp", "permissions", "list"]);
    assert!(listed.status.success(), "stderr: {}", stderr(&listed));
    assert_eq!(
        stdout(&listed),
        "mcp.fake.send\texternal_connector\tallow\n"
    );

    let forgotten = run_codel00p(
        &db_path,
        &[
            "mcp",
            "permissions",
            "forget",
            "mcp.fake.send",
            "--scope",
            "external_connector",
        ],
    );
    assert!(forgotten.status.success(), "stderr: {}", stderr(&forgotten));
    assert_eq!(
        stdout(&forgotten),
        "forgot\tmcp.fake.send\texternal_connector\n"
    );

    let listed_after_forget = run_codel00p(&db_path, &["mcp", "permissions", "list"]);
    assert!(
        listed_after_forget.status.success(),
        "stderr: {}",
        stderr(&listed_after_forget)
    );
    assert_eq!(stdout(&listed_after_forget), "");
}

#[test]
fn agent_run_remembers_denied_mcp_connector_permissions() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    let mcp_server = dir.path().join("fake-denied-remembered-mcp.sh");
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

    let first_provider = MockServer::start();
    let first_initial = first_provider.mock(|when, then| {
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
                                "id": "call-send-denied",
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
    let first_final = first_provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("permission_denied")
            .body_includes("mcp.fake.send rejected by CLI approval prompt");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Denied once."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let first = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "run",
            "Send once.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &first_provider.base_url(),
            "--mcp-server",
            &format!("fake={}", mcp_server.display()),
            "--permission-mode",
            "ask",
            "--remember-permissions",
        ],
        "n\n",
    );

    assert!(first.status.success(), "stderr: {}", stderr(&first));
    assert_eq!(stdout(&first), "Denied once.\n");
    first_initial.assert();
    first_final.assert();

    let second_provider = MockServer::start();
    let second_initial = second_provider.mock(|when, then| {
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
                                "id": "call-send-denied-again",
                                "type": "function",
                                "function": {
                                    "name": "mcp.fake.send",
                                    "arguments": "{\"text\":\"hello again\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ]
        }));
    });
    let second_final = second_provider.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""role":"tool""#)
            .body_includes("permission_denied")
            .body_includes("mcp.fake.send denied by remembered connector policy");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Still denied."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let second = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Send again.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &second_provider.base_url(),
            "--mcp-server",
            &format!("fake={}", mcp_server.display()),
            "--permission-mode",
            "ask",
            "--remember-permissions",
        ],
    );

    assert!(second.status.success(), "stderr: {}", stderr(&second));
    assert_eq!(stdout(&second), "Still denied.\n");
    assert!(
        !stderr(&second).contains("Allow tool"),
        "stderr: {}",
        stderr(&second)
    );
    second_initial.assert();
    second_final.assert();
}
