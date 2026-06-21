use super::support::*;

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
    // This test exercises the permission approval flow on a single mutating tool
    // call, so keep the (default-on) verify-before-done self-critique out of the
    // way: it would otherwise add a reflection inference after the mutating turn.
    fs::create_dir(workspace.join(".codel00p")).expect("create .codel00p");
    fs::write(
        workspace.join(".codel00p/config.toml"),
        "[agent.behavior]\nself_critique = false\n",
    )
    .expect("write config");

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
