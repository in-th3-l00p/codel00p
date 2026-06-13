use super::fixtures::*;

#[tokio::test]
async fn git_status_requests_read_only_permission() {
    let dir = tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-status", "git_status", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Status checked."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::git_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-git-status"),
            UserMessage::new("Check git status."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::ReadOnly,
                ..
            } if tool_name == "git_status"
        )
    }));
}

#[tokio::test]
async fn git_commit_requests_workspace_write_permission() {
    let dir = tempdir().expect("tempdir");
    init_git_repo(dir.path());
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-commit",
                "git_commit",
                json!({ "message": "change tracked file" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Committed."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::git_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-git-commit"),
            UserMessage::new("Commit the change."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::WorkspaceWrite,
                ..
            } if tool_name == "git_commit"
        )
    }));
    assert_eq!(
        outcome.tool_calls[0].result.content()["subject"],
        "change tracked file"
    );
}
