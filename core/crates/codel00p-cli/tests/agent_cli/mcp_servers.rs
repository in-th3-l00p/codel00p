use super::support::*;

#[test]
fn agent_run_can_attach_stdio_mcp_servers_as_tools() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    let mcp_server = dir.path().join("fake-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"Echo text.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}'
read call
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"echoed"}],"isError":false}}'
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
            .body_includes(r#""name":"mcp.fake.echo""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-echo",
                                "type": "function",
                                "function": {
                                    "name": "mcp.fake.echo",
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
            .body_includes("echoed")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "MCP echoed."
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
            "Use the MCP echo tool.",
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
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "MCP echoed.\n");
    first.assert();
    second.assert();
}

#[test]
fn agent_run_mcp_server_specs_support_args_and_env_assignments() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    let mcp_server = dir.path().join("fake-mcp-with-args.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
TOOL_NAME="$1"
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"%s","description":"Echo text.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}\n' "$TOOL_NAME"
read call
printf '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"%s"}],"isError":false}}\n' "$CODEL00P_FAKE_ECHO"
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
            .body_includes(r#""name":"mcp.fake.echo_arg""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-echo",
                                "type": "function",
                                "function": {
                                    "name": "mcp.fake.echo_arg",
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
            .body_includes("env echo")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "MCP args and env worked."
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
            "Use the MCP echo tool.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--mcp-server",
            &format!(
                "fake=CODEL00P_FAKE_ECHO='env echo' {} echo_arg",
                mcp_server.display()
            ),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "MCP args and env worked.\n");
    first.assert();
    second.assert();
}

#[test]
fn agent_run_loads_stdio_mcp_servers_from_workspace_config() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::create_dir(workspace.join(".codel00p")).expect("create config dir");
    let mcp_server = workspace.join("fake-config-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
TOOL_NAME="$1"
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"%s","description":"Echo text.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}\n' "$TOOL_NAME"
read call
printf '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"%s"}],"isError":false}}\n' "$CODEL00P_FAKE_ECHO"
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
                "fake": {
                    "command": "./fake-config-mcp.sh",
                    "args": ["echo_config"],
                    "env": {
                        "CODEL00P_FAKE_ECHO": "config echo"
                    },
                    "timeoutMs": 5000
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
            .body_includes(r#""name":"mcp.fake.echo_config""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-echo",
                                "type": "function",
                                "function": {
                                    "name": "mcp.fake.echo_config",
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
            .body_includes("config echo")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "MCP config worked."
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
            "Use the configured MCP echo tool.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "MCP config worked.\n");
    first.assert();
    second.assert();
}

#[test]
fn agent_mcp_list_prints_configured_stdio_tools() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::create_dir(workspace.join(".codel00p")).expect("create config dir");
    let mcp_server = workspace.join("fake-list-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search","description":"Search docs.","inputSchema":{"type":"object"}},{"name":"open","description":"Open docs.","inputSchema":{"type":"object"}}]}}'
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
                    "command": "./fake-list-mcp.sh"
                }
            }
        })
        .to_string(),
    )
    .expect("write mcp config");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "mcp",
            "list",
            "--workspace",
            workspace.to_str().expect("workspace path"),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "mcp.docs.open\tOpen docs.\nmcp.docs.search\tSearch docs.\n"
    );
}

#[test]
fn agent_mcp_list_prints_configured_http_tools() {
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
            .header("authorization", "Bearer test-token")
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
    fs::write(
        workspace.join(".codel00p/mcp.json"),
        json!({
            "servers": {
                "docs": {
                    "url": format!("{}/mcp", mcp.base_url()),
                    "bearerTokenEnv": "CODEL00P_PROVIDER_CUSTOM_API_KEY"
                }
            }
        })
        .to_string(),
    )
    .expect("write mcp config");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "mcp",
            "list",
            "--workspace",
            workspace.to_str().expect("workspace path"),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "mcp.docs.lookup\tLookup remote docs.\n");
    initialize.assert();
    initialized.assert();
    list.assert();
}

#[test]
fn agent_mcp_doctor_reports_capabilities_without_leaking_env_values() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::create_dir(workspace.join(".codel00p")).expect("create config dir");
    let mcp_server = workspace.join("fake-doctor-mcp.sh");
    fs::write(
        &mcp_server,
        r#"#!/bin/sh
case "$SECRET_TOKEN" in super-secret) ;; *) exit 31;; esac
read init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{},"resources":{},"prompts":{}},"serverInfo":{"name":"fake","version":"1.0.0"}}}'
read initialized
read tools
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search","description":"Search docs.","inputSchema":{"type":"object"}}]}}'
read resources
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"resources":[{"uri":"codel00p://docs/one","name":"One","mimeType":"text/plain"}]}}'
read prompts
printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{"prompts":[{"name":"review","description":"Review docs."}]}}'
"#,
    )
    .expect("write fake mcp server");
    fs::write(
        workspace.join(".codel00p/mcp.json"),
        json!({
            "servers": {
                "docs": {
                    "command": "/bin/sh",
                    "args": [mcp_server.to_str().expect("mcp server path")],
                    "env": {
                        "SECRET_TOKEN": "super-secret"
                    },
                    "timeoutMs": 500
                }
            }
        })
        .to_string(),
    )
    .expect("write mcp config");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "mcp",
            "doctor",
            "--workspace",
            workspace.to_str().expect("workspace path"),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(
        stdout.contains("docs\tok\tstdio\ttools=1\tresources=1\tprompts=1"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("env=SECRET_TOKEN:<redacted>"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("timeout_ms=500"), "stdout: {stdout}");
    assert!(!stdout.contains("super-secret"), "stdout: {stdout}");
}
