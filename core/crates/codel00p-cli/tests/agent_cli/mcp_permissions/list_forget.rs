use crate::support::*;

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
