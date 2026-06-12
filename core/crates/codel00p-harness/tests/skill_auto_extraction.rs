mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, HarnessError, HarnessInferenceResponse, ModelToolCall, ProcedureSkillExtractor,
    ProposedSkill, SessionId, SkillProposalSink, ToolRegistry, UserMessage, Workspace,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

struct RecordingSink {
    proposals: Arc<Mutex<Vec<ProposedSkill>>>,
}

#[async_trait]
impl SkillProposalSink for RecordingSink {
    async fn propose(&self, skill: ProposedSkill) -> Result<(), HarnessError> {
        self.proposals.lock().expect("lock").push(skill);
        Ok(())
    }
}

// A turn that carries out a real procedure (two file writes) then answers should
// auto-propose one draft skill capturing the goal and the steps.
#[tokio::test]
async fn completed_procedure_auto_proposes_a_skill() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "custom",
            "test-model",
            vec![
                ModelToolCall::new(
                    "c1",
                    "create_file",
                    json!({ "path": "a.txt", "content": "a" }),
                ),
                ModelToolCall::new(
                    "c2",
                    "create_file",
                    json!({ "path": "b.txt", "content": "b" }),
                ),
            ],
        ),
        HarnessInferenceResponse::assistant("custom", "test-model", "Done."),
    ]);

    let proposals = Arc::new(Mutex::new(Vec::new()));
    let sink = RecordingSink {
        proposals: proposals.clone(),
    };

    AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .skill_extractor(ProcedureSkillExtractor::default())
        .skill_proposal_sink(sink)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("s"),
            UserMessage::new("Scaffold the project files"),
        )
        .await
        .expect("run turn");

    let recorded = proposals.lock().expect("lock");
    assert_eq!(recorded.len(), 1, "one draft skill should be proposed");
    assert_eq!(recorded[0].name(), "scaffold-the-project-files");
    assert!(recorded[0].instructions().contains("create_file"));
}

// A turn that only reads should not propose anything.
#[tokio::test]
async fn read_only_turn_proposes_nothing() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.txt"), "hello").expect("seed file");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "custom",
            "test-model",
            vec![ModelToolCall::new(
                "c1",
                "read_file",
                json!({ "path": "a.txt" }),
            )],
        ),
        HarnessInferenceResponse::assistant("custom", "test-model", "Read it."),
    ]);

    let proposals = Arc::new(Mutex::new(Vec::new()));
    AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .skill_extractor(ProcedureSkillExtractor::default())
        .skill_proposal_sink(RecordingSink {
            proposals: proposals.clone(),
        })
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("s2"),
            UserMessage::new("Read the file"),
        )
        .await
        .expect("run turn");

    assert!(proposals.lock().expect("lock").is_empty());
}
