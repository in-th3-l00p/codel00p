use crate::support::*;

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
