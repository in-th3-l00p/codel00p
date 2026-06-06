mod support;

use std::fs;

use codel00p_harness::{
    AgentHarness, HarnessError, HarnessEvent, HarnessInferenceResponse, ModelToolCall, SessionId,
    SessionMessage, ToolRegistry, UserMessage, Workspace,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

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
            SessionMessage::tool(
                "call-1",
                "read_file",
                r#"{"content":"Agent Harness\n","path":"README.md"}"#,
            ),
            SessionMessage::assistant("README contains Agent Harness."),
        ]
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallCompleted { name } if name == "read_file")
    }));
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { iterations: 2 })
    ));
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
        matches!(event, HarnessEvent::ToolCallFailed { name, .. } if name == "missing")
    }));
}

#[tokio::test]
async fn iteration_limit_stops_infinite_tool_call_loop() {
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
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-2",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
    ]);

    let error = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(1)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-limit"),
            UserMessage::new("Loop."),
        )
        .await
        .expect_err("iteration limit should fail");

    assert!(matches!(error, HarnessError::IterationLimit { limit: 1 }));
}
