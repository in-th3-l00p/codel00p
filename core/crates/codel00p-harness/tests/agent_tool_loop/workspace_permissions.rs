use super::fixtures::*;

#[tokio::test]
async fn write_tool_calls_request_workspace_write_permission_and_mutate_when_allowed() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-create",
                "create_file",
                json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 42 }\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Created the file."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-write-tool"),
            UserMessage::new("Create src/lib.rs."),
        )
        .await
        .expect("run turn");

    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).expect("read created file"),
        "pub fn answer() -> u32 { 42 }\n"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::WorkspaceWrite,
                ..
            } if tool_name == "create_file"
        )
    }));
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallCompleted { tool_name, .. } if tool_name == "create_file")
    }));
}

#[tokio::test]
async fn denied_write_tool_calls_do_not_mutate_workspace() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-create-denied",
                "create_file",
                json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 42 }\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Write was denied."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .permission_policy(DenyToolPolicy)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-write-denied"),
            UserMessage::new("Create src/lib.rs."),
        )
        .await
        .expect("run turn");

    assert!(!dir.path().join("src/lib.rs").exists());
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "create_file")
    }));
}

#[tokio::test]
async fn command_tool_calls_request_shell_permission_and_capture_output() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-command",
                "run_command",
                json!({
                    "program": "sh",
                    "args": ["-c", "printf command-output"],
                    "timeout_ms": 1000
                }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Command finished."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::command_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-command-tool"),
            UserMessage::new("Run a command."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::Shell,
                ..
            } if tool_name == "run_command"
        )
    }));
    assert_eq!(
        outcome.tool_calls[0].result.content()["stdout"],
        "command-output"
    );
    assert_eq!(outcome.tool_calls[0].result.content()["success"], true);
}

#[tokio::test]
async fn denied_command_tool_calls_do_not_execute() {
    let dir = tempdir().expect("tempdir");
    let marker = dir.path().join("marker.txt");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-command-denied",
                "run_command",
                json!({
                    "program": "sh",
                    "args": ["-c", "printf denied > marker.txt"],
                    "timeout_ms": 1000
                }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Command denied."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::command_defaults())
        .permission_policy(DenyToolPolicy)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-command-denied"),
            UserMessage::new("Run a denied command."),
        )
        .await
        .expect("run turn");

    assert!(!marker.exists());
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "run_command")
    }));
}
