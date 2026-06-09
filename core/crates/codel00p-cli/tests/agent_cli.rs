use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use httpmock::{Method::POST, MockServer};
use serde_json::json;
use tempfile::tempdir;

fn run_codel00p(db_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .output()
        .expect("run codel00p")
}

fn run_codel00p_without_provider_env(db_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .env_remove("CODEL00P_PROVIDER_CUSTOM_API_KEY")
        .env_remove("CODEL00P_PROVIDER_OPENAI_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .args(args)
        .output()
        .expect("run codel00p")
}

fn run_codel00p_with_stdin(db_path: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn codel00p");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");

    child.wait_with_output().expect("run codel00p")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

#[test]
fn agent_run_calls_provider_with_read_only_tools_and_prints_final_text() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .header("authorization", "Bearer test-token")
            .body_includes(r#""model":"test-model""#)
            .body_includes(r#""role":"user""#)
            .body_includes(r#""content":"Inspect this project.""#)
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "The project has a README."
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
            "Inspect this project.",
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
    assert_eq!(stdout(&output), "The project has a README.\n");
    provider.assert();
}

#[test]
fn agent_run_defaults_to_read_only_tools() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#)
            .body_excludes(r#""name":"create_file""#)
            .body_excludes(r#""name":"run_command""#)
            .body_excludes(r#""name":"git_status""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Read-only tools only."
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
            "Inspect tools.",
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
    assert_eq!(stdout(&output), "Read-only tools only.\n");
    provider.assert();
}

#[test]
fn agent_run_can_opt_into_edit_command_and_git_tool_sets() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"create_file""#)
            .body_includes(r#""name":"apply_patch""#)
            .body_includes(r#""name":"run_command""#)
            .body_includes(r#""name":"git_status""#)
            .body_includes(r#""name":"git_commit""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Loaded expanded tools."
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
            "Inspect tools.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "edit",
            "--tool-set",
            "command",
            "--tool-set",
            "git",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Loaded expanded tools.\n");
    provider.assert();
}

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
fn agent_run_rejects_unknown_tool_sets() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect tools.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
            "--tool-set",
            "danger",
        ],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown tool set: danger"));
}

#[test]
fn agent_run_rejects_invalid_mcp_server_specs() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect tools.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
            "--mcp-server",
            "broken",
        ],
    );

    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("invalid --mcp-server, expected <id>=<command>"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn agent_run_denies_tool_execution_when_permission_mode_is_deny() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"create_file""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-create",
                                "type": "function",
                                "function": {
                                    "name": "create_file",
                                    "arguments": "{\"path\":\"created.txt\",\"content\":\"nope\"}"
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
            .body_includes("create_file denied by CLI permission mode");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Write denied."
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
            "Create the file.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "edit",
            "--permission-mode",
            "deny",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Write denied.\n");
    assert!(!workspace.join("created.txt").exists());
    first.assert();
    second.assert();
}

#[test]
fn agent_run_fails_closed_when_permission_mode_is_ask_without_interactive_prompt() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"create_file""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-create",
                                "type": "function",
                                "function": {
                                    "name": "create_file",
                                    "arguments": "{\"path\":\"created.txt\",\"content\":\"nope\"}"
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
            .body_includes("rejected by CLI approval prompt");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Approval unavailable."
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
            "Create the file.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "edit",
            "--permission-mode",
            "ask",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Approval unavailable.\n");
    assert!(!workspace.join("created.txt").exists());
    first.assert();
    second.assert();
}

#[test]
fn agent_run_allows_tool_execution_when_permission_mode_ask_is_approved() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"create_file""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-create",
                                "type": "function",
                                "function": {
                                    "name": "create_file",
                                    "arguments": "{\"path\":\"created.txt\",\"content\":\"created\"}"
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
            .body_includes("created.txt")
            .body_includes("bytes_written")
            .body_excludes("permission_denied");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Write approved."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "run",
            "Create the file.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "edit",
            "--permission-mode",
            "ask",
        ],
        "y\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Write approved.\n");
    assert_eq!(
        fs::read_to_string(workspace.join("created.txt")).expect("created file"),
        "created"
    );
    assert!(stderr(&output).contains("Allow tool `create_file`"));
    first.assert();
    second.assert();
}

#[test]
fn agent_run_denies_tool_execution_when_permission_mode_ask_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""name":"create_file""#)
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call-create",
                                "type": "function",
                                "function": {
                                    "name": "create_file",
                                    "arguments": "{\"path\":\"created.txt\",\"content\":\"nope\"}"
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
            .body_includes("rejected by CLI approval prompt");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Write rejected."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "run",
            "Create the file.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--tool-set",
            "edit",
            "--permission-mode",
            "ask",
        ],
        "n\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Write rejected.\n");
    assert!(!workspace.join("created.txt").exists());
    first.assert();
    second.assert();
}

#[test]
fn agent_run_rejects_unknown_permission_modes() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect tools.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
            "--permission-mode",
            "maybe",
        ],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown permission mode: maybe"));
}

#[test]
fn agent_run_can_stream_json_events_before_final_text() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Streaming complete."
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
            "Stream events.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--stream-events",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.len() >= 2, "stdout: {stdout}");
    assert!(lines[0].contains(r#""kind":"turn_started""#));
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""kind":"turn_completed""#))
    );
    assert_eq!(lines.last(), Some(&"Streaming complete."));
    provider.assert();
}

#[test]
fn agent_run_persists_session_messages_and_events() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Session persistence works."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Persist this turn.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "session-cli",
        ],
    );
    let show = run_codel00p(&db_path, &["session", "show", "session-cli"]);

    assert!(run.status.success(), "stderr: {}", stderr(&run));
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert_eq!(stdout(&run), "Session persistence works.\n");
    let session_output = stdout(&show);
    assert!(session_output.contains("1\tmessage\tuser\tPersist this turn.\n"));
    assert!(session_output.contains("2\tmessage\tassistant\tSession persistence works.\n"));
    assert!(session_output.contains("\tevent\tturn_started\t\n"));
    assert!(session_output.contains("\tevent\tturn_completed\t\n"));
    provider.assert();
}

#[test]
fn agent_resume_replays_prior_session_messages_and_appends_only_new_records() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let first_server = MockServer::start();
    let first = first_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Initial request.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Initial answer."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let first_run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Initial request.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &first_server.base_url(),
            "--session-id",
            "session-resume",
        ],
    );
    assert!(first_run.status.success(), "stderr: {}", stderr(&first_run));
    first.assert();

    let second_server = MockServer::start();
    let second = second_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Initial request.")
            .body_includes("Initial answer.")
            .body_includes("Continue with the next step.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Resumed answer."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let resumed = run_codel00p(
        &db_path,
        &[
            "agent",
            "resume",
            "session-resume",
            "Continue with the next step.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &second_server.base_url(),
        ],
    );
    assert!(resumed.status.success(), "stderr: {}", stderr(&resumed));
    assert_eq!(stdout(&resumed), "Resumed answer.\n");
    second.assert();

    let show = run_codel00p(&db_path, &["session", "show", "session-resume"]);
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    let session_output = stdout(&show);
    assert_eq!(
        occurrences(&session_output, "\tmessage\tuser\tInitial request.\n"),
        1
    );
    assert_eq!(
        occurrences(&session_output, "\tmessage\tassistant\tInitial answer.\n"),
        1
    );
    assert_eq!(
        occurrences(
            &session_output,
            "\tmessage\tuser\tContinue with the next step.\n"
        ),
        1
    );
    assert_eq!(
        occurrences(&session_output, "\tmessage\tassistant\tResumed answer.\n"),
        1
    );
}

#[test]
fn agent_run_extracts_reviewed_memory_and_reuses_approved_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Capture memory.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "remember workflow[verify]: Run pnpm verify before pushing main."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let first_run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Capture memory.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "session-memory",
        ],
    );
    assert!(first_run.status.success(), "stderr: {}", stderr(&first_run));
    first.assert();

    let listed = run_codel00p(&db_path, &["memory", "list", "--status", "candidate"]);
    assert!(listed.status.success(), "stderr: {}", stderr(&listed));
    let listed_stdout = stdout(&listed);
    assert!(
        listed_stdout.contains("\tcandidate\tworkflow\tRun pnpm verify before pushing main.\n")
    );
    let memory_id = listed_stdout
        .split('\t')
        .next()
        .expect("memory id")
        .to_string();

    let approve = run_codel00p(
        &db_path,
        &["memory", "approve", &memory_id, "--actor", "alice"],
    );
    assert!(approve.status.success(), "stderr: {}", stderr(&approve));

    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Use memory.")
            .body_includes("Project memory:")
            .body_includes("kind: workflow")
            .body_includes("Run pnpm verify before pushing main.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Loaded reviewed project memory."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let second_run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Use memory.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "session-memory-2",
        ],
    );

    assert!(
        second_run.status.success(),
        "stderr: {}",
        stderr(&second_run)
    );
    assert_eq!(stdout(&second_run), "Loaded reviewed project memory.\n");
    second.assert();
}

#[test]
fn agent_run_missing_credential_lists_supported_environment_variables() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p_without_provider_env(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
        ],
    );

    assert!(!output.status.success());
    let error = stderr(&output);
    assert!(error.contains("missing credential for provider `custom`"));
    assert!(error.contains("CODEL00P_PROVIDER_CUSTOM_API_KEY"));
}

#[test]
fn agent_run_rejects_provider_modes_that_are_not_implemented_for_cli_agent() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p_without_provider_env(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "openai",
            "--model",
            "gpt-5",
        ],
    );

    assert!(!output.status.success());
    let error = stderr(&output);
    assert!(error.contains("provider `openai` uses responses"));
    assert!(error.contains("agent run currently supports chat_completions providers"));
}
