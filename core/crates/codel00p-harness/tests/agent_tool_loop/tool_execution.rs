use super::fixtures::*;

#[tokio::test]
async fn executes_tool_calls_and_continues_until_final_text() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "README contains Agent Harness."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-tools"),
            UserMessage::new("Read README."),
        )
        .await
        .expect("run turn");

    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("README contains Agent Harness.")
    );
    assert_eq!(model.requests().len(), 2);
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "read_file");
    assert_eq!(
        outcome.session_state.messages(),
        &[
            SessionMessage::user("Read README."),
            SessionMessage::assistant_tool_calls(vec![ModelToolCall::new(
                "call-1",
                "read_file",
                json!({ "path": "README.md" }),
            )]),
            SessionMessage::tool_result(
                "call-1",
                "read_file",
                r#"{"content":"Agent Harness\n","path":"README.md"}"#,
            ),
            SessionMessage::assistant("README contains Agent Harness."),
        ]
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallCompleted { tool_name, .. } if tool_name == "read_file")
    }));
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { iterations: 2, .. })
    ));
}

#[tokio::test]
async fn tool_result_progress_is_emitted_as_harness_events() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-1", "progress_tool", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(ProgressTool))
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-tool-progress"),
            UserMessage::new("Use progress tool."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::ToolProgress {
                tool_name,
                phase,
                message: Some(message),
                ..
            } if tool_name == "progress_tool"
                && phase == "mcp_progress"
                && message == "Half done"
        )
    }));
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::ToolProgress {
                tool_name,
                phase,
                message: Some(message),
                ..
            } if tool_name == "progress_tool"
                && phase == "mcp_resource_updated"
                && message == "codel00p://memory/mem-1"
        )
    }));
}

#[tokio::test]
async fn unknown_tool_call_is_recorded_as_failed_tool_result() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-missing", "missing", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "The tool was unavailable."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-missing"),
            UserMessage::new("Use missing."),
        )
        .await
        .expect("run turn");

    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "missing");
    assert!(
        outcome.tool_calls[0].result.content()["error"]
            .as_str()
            .unwrap()
            .contains("tool not found")
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallFailed { tool_name, .. } if tool_name == "missing")
    }));
}

#[tokio::test]
async fn denied_tool_call_is_recorded_without_executing_tool() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-denied", "counting", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Tool was denied."),
    ]);
    let executions = Arc::new(AtomicUsize::new(0));

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(CountingTool {
            executions: executions.clone(),
        }))
        .permission_policy(DenyToolPolicy)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-denied"),
            UserMessage::new("Try denied tool."),
        )
        .await
        .expect("run turn");

    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "counting");
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "counting")
    }));
}
