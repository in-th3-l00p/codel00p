use codel00p_harness::{
    AgentHarness, HarnessInferenceResponse, ProjectInstructionLoader, SessionId, ToolRegistry,
    UserMessage, Workspace,
};
use tempfile::tempdir;

mod support;

use support::ScriptedModelClient;

#[test]
fn loader_reads_root_instruction_files_in_deterministic_precedence() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("CLAUDE.md"),
        "Claude compatibility rules.\n",
    )
    .expect("write claude");
    std::fs::write(dir.path().join("AGENTS.md"), "Agent compatibility rules.\n")
        .expect("write agents");
    std::fs::write(dir.path().join("CODEL00P.md"), "Native codel00p rules.\n")
        .expect("write codel00p");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let instructions = ProjectInstructionLoader
        .load(&workspace)
        .expect("load instructions");

    assert_eq!(
        instructions.sources(),
        &["CODEL00P.md", "AGENTS.md", "CLAUDE.md"]
    );
    assert_eq!(
        instructions.as_prompt(),
        "\
Project instructions:
## CODEL00P.md
Native codel00p rules.

## AGENTS.md
Agent compatibility rules.

## CLAUDE.md
Claude compatibility rules."
    );
}

#[tokio::test]
async fn harness_attaches_project_instructions_to_inference_request() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("CODEL00P.md"), "Always run pnpm verify.\n")
        .expect("write instructions");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Understood.",
    )]);

    AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-instructions"),
            UserMessage::new("Inspect."),
        )
        .await
        .expect("run turn");

    let requests = model.requests();
    let instructions = requests[0]
        .project_instructions()
        .expect("project instructions");
    assert_eq!(instructions.sources(), &["CODEL00P.md"]);
    assert_eq!(
        instructions.as_prompt(),
        "\
Project instructions:
## CODEL00P.md
Always run pnpm verify."
    );
}
