//! End-to-end tests for the default sub-agent spawner: a delegated task runs as
//! an isolated child `AgentHarness` turn and its summary flows back to the parent.

mod support;

use std::sync::Arc;

use codel00p_harness::{
    AgentHarness, AgentRole, DelegatedTask, HarnessInferenceResponse, HarnessSubAgentSpawner,
    ModelToolCall, SessionId, SubAgentSpawner, ToolRegistry, UserMessage, Workspace,
    delegation_tools,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

#[tokio::test]
async fn spawner_runs_child_and_returns_its_summary() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    // The child model produces a final text summary with no tool calls.
    let child_model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "the child's summary",
    )]);

    let spawner = HarnessSubAgentSpawner::new(
        Arc::new(child_model),
        workspace,
        ToolRegistry::read_only_defaults(),
    );

    let outcome = spawner
        .spawn(DelegatedTask::new("summarize the project"))
        .await
        .expect("spawn child");

    assert_eq!(outcome.summary(), "the child's summary");
    assert_eq!(outcome.tool_calls(), 0);
    // Each child gets its own isolated session.
    assert!(outcome.child_session_id().as_str().starts_with("session-"));
}

#[tokio::test]
async fn child_runs_tools_before_summarizing() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    // The child reads a file, then summarizes.
    let child_model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "c1",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "README mentions Agent Harness."),
    ]);

    let spawner = HarnessSubAgentSpawner::new(
        Arc::new(child_model),
        workspace,
        ToolRegistry::read_only_defaults(),
    );

    let outcome = spawner
        .spawn(DelegatedTask::new("read and summarize the README"))
        .await
        .expect("spawn child");

    assert_eq!(outcome.summary(), "README mentions Agent Harness.");
    assert_eq!(outcome.tool_calls(), 1);
}

#[tokio::test]
async fn orchestrator_delegates_to_a_child_end_to_end() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    // The child agent (its own model) returns a summary.
    let child_model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "child finished the subtask",
    )]);
    let spawner = Arc::new(HarnessSubAgentSpawner::new(
        Arc::new(child_model),
        workspace.clone(),
        ToolRegistry::read_only_defaults(),
    ));

    // The orchestrator agent calls delegate_task, then reports done.
    let parent_model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "d1",
                "delegate_task",
                json!({ "task": "do the subtask", "label": "sub" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "orchestration complete"),
    ]);

    // Orchestrator tools = read-only plus the delegate_task tool.
    let tools = ToolRegistry::read_only_defaults()
        .with_registry(delegation_tools(AgentRole::Orchestrator, spawner));

    let outcome = AgentHarness::builder()
        .model_client(parent_model)
        .workspace(workspace)
        .tools(tools)
        .build()
        .expect("build orchestrator")
        .run_turn(
            SessionId::from_static("parent"),
            UserMessage::new("Delegate the subtask."),
        )
        .await
        .expect("run orchestrator turn");

    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("orchestration complete")
    );
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "delegate_task");
    // The child's summary came back through the delegate_task result.
    assert_eq!(
        outcome.tool_calls[0].result.content()["summary"],
        json!("child finished the subtask")
    );
}
