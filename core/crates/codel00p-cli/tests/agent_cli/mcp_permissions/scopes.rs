use crate::support::*;

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
