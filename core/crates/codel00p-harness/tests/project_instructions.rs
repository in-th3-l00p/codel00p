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

#[test]
fn loader_with_no_instruction_files_is_empty() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let instructions = ProjectInstructionLoader
        .load(&workspace)
        .expect("load instructions");

    assert!(instructions.is_empty());
}

#[test]
fn loader_walks_ancestor_directories_with_root_most_specific() {
    let dir = tempdir().expect("tempdir");
    // ancestor (parent) holds an instruction file; the workspace root is a child.
    std::fs::write(dir.path().join("CODEL00P.md"), "Ancestor rules.\n").expect("write ancestor");
    let root = dir.path().join("project");
    std::fs::create_dir(&root).expect("mkdir project");
    std::fs::write(root.join("CODEL00P.md"), "Root rules.\n").expect("write root");
    let workspace = Workspace::new(&root).expect("workspace");

    let instructions = ProjectInstructionLoader
        .load(&workspace)
        .expect("load instructions");

    let sources = instructions.sources();
    // Root file is first (most specific); the ancestor copy is appended and
    // disambiguated by its directory.
    assert_eq!(sources.len(), 2, "sources: {sources:?}");
    assert_eq!(sources[0], "CODEL00P.md");
    assert!(
        sources[1].starts_with("CODEL00P.md ("),
        "ancestor source should be disambiguated: {sources:?}"
    );
    let prompt = instructions.as_prompt();
    let root_idx = prompt.find("Root rules.").expect("root rules present");
    let ancestor_idx = prompt.find("Ancestor rules.").expect("ancestor present");
    assert!(
        root_idx < ancestor_idx,
        "root (most specific) must render before ancestor: {prompt}"
    );
}

#[test]
fn loader_root_only_project_is_unchanged_by_ancestor_walk() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join("project");
    std::fs::create_dir(&root).expect("mkdir project");
    std::fs::write(root.join("CODEL00P.md"), "Root rules.\n").expect("write root");
    let workspace = Workspace::new(&root).expect("workspace");

    let instructions = ProjectInstructionLoader
        .load(&workspace)
        .expect("load instructions");

    // No instruction files in any ancestor dir -> exactly the root file.
    assert_eq!(instructions.sources(), &["CODEL00P.md"]);
    assert_eq!(
        instructions.as_prompt(),
        "\
Project instructions:
## CODEL00P.md
Root rules."
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
