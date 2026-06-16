//! End-to-end test: the harness advertises `run_pipeline` when programmatic
//! tooling is enabled, and a single pipeline tool call runs a multi-step,
//! governed pipeline in one inference.

mod support;

use codel00p_harness::{
    AgentHarness, HarnessError, HarnessInferenceResponse, ModelToolCall, PermissionDecision,
    PermissionMode, PermissionPolicy, PermissionRequest, SessionId, ToolRegistry, UserMessage,
    Workspace,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

/// Denies `create_file`, allows everything else — proves per-step governance
/// holds inside a pipeline.
struct DenyCreate;

#[async_trait::async_trait]
impl PermissionPolicy for DenyCreate {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        if request.tool_name() == "create_file" {
            Ok(PermissionDecision::deny(
                request.id(),
                PermissionMode::Deny,
                "create blocked",
            ))
        } else {
            Ok(PermissionDecision::allow(
                request.id(),
                PermissionMode::Allow,
            ))
        }
    }
}

#[tokio::test]
async fn harness_runs_a_pipeline_in_one_turn() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("src.txt"), "PAYLOAD").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "run_pipeline",
                json!({
                    "steps": [
                        { "tool": "read_file", "id": "src", "input": { "path": "src.txt" } },
                        {
                            "tool": "create_file",
                            "input": { "path": "out.txt", "content": "{{src.content}}" }
                        },
                        { "tool": "read_file", "input": { "path": "out.txt" } }
                    ]
                }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Pipeline done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults().with_registry(ToolRegistry::editing_defaults()))
        .programmatic_tooling(true)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-pipeline"),
            UserMessage::new("Copy src to out."),
        )
        .await
        .expect("run turn");

    // The whole multi-step pipeline ran in a single tool call / inference.
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "run_pipeline");

    let content = outcome.tool_calls[0].result.content();
    assert_eq!(content["completed"], 3);
    let steps = content["steps"].as_array().unwrap();
    assert_eq!(steps[2]["output"]["content"], "PAYLOAD");

    // `run_pipeline` is advertised to the model.
    let advertised = model.requests()[0].tool_names().to_vec();
    assert!(advertised.contains(&"run_pipeline".to_string()));
    // The base tools are still advertised alongside it.
    assert!(advertised.contains(&"read_file".to_string()));
}

#[tokio::test]
async fn pipeline_respects_the_harness_permission_policy() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "run_pipeline",
                json!({
                    "stop_on_error": false,
                    "steps": [
                        { "tool": "create_file", "input": { "path": "x.txt", "content": "y" } }
                    ]
                }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults().with_registry(ToolRegistry::editing_defaults()))
        .permission_policy(DenyCreate)
        .programmatic_tooling(true)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-pipeline-deny"),
            UserMessage::new("Try to create a file."),
        )
        .await
        .expect("run turn");

    let content = outcome.tool_calls[0].result.content();
    assert_eq!(content["completed"], 0);
    assert_eq!(content["steps"][0]["ok"], false);
    // The denied write never happened, even though the pipeline tool itself ran.
    assert!(!dir.path().join("x.txt").exists());
}
