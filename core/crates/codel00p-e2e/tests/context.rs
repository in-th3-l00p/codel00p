//! End-to-end context-assembly scenarios.
//!
//! Verifies that the real `codel00p` binary correctly assembles the context that
//! the model receives: project-instruction files, approved memory, skill
//! injection, manifest determinism, and (where feasible headlessly) context
//! compaction.
//!
//! All scenarios use the [`CodelRunner`] / [`MockProvider`] harness — fully
//! hermetic, no network, isolated `CODEL00P_HOME` + workspace per test.
//!
//! # Headlessly-infeasible scenarios (documented here, not faked)
//!
//! - **Context compaction** (`ContextCompacted` event): compaction is triggered
//!   when the session's accumulated message count exceeds the harness compaction
//!   threshold (`compaction_threshold` in `AgentHarness::builder()`). That
//!   threshold is **not** a CLI flag (verified: no `--compaction-threshold` match
//!   in `codel00p-cli/src`), and the default is large enough that a single-turn
//!   headless run can never reach it. Compaction is therefore covered at the
//!   harness unit-test layer (`codel00p-harness/tests/`) and **not** asserted
//!   here. This comment is the required headless-feasibility documentation.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use codel00p_memory::{MemoryListFilter, MemoryRepository, StorageBackedMemoryStore};
use codel00p_protocol::{MemoryStatus, ProjectRef};
use codel00p_storage::{SqliteStorage, StorageScope};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Lists persisted memory candidates via the real memory repository types.
fn list_memory_with_status(db: &std::path::Path, status: MemoryStatus) -> Vec<String> {
    let storage = SqliteStorage::open(db).expect("open memory sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let project = ProjectRef::new("project-1", "codel00p");
    let filter = MemoryListFilter::new(project).with_status(status);
    store
        .list(filter)
        .expect("list memory")
        .into_iter()
        .map(|record| record.entry().id().to_string())
        .collect()
}

/// Extract the `content_hash` from a `ContextManifest` event, panicking if it
/// is not the right variant.
fn manifest_content_hash(event: &AgentEvent) -> String {
    match event {
        AgentEvent::ContextManifest { content_hash, .. } => content_hash.clone(),
        other => panic!("expected ContextManifest, got {other:?}"),
    }
}

/// Extract `instruction_sources` from a `ContextManifest` event.
fn manifest_instruction_sources(event: &AgentEvent) -> Vec<String> {
    match event {
        AgentEvent::ContextManifest {
            instruction_sources,
            ..
        } => instruction_sources.clone(),
        other => panic!("expected ContextManifest, got {other:?}"),
    }
}

/// Extract `injected_memory_ids` from a `ContextManifest` event.
fn manifest_injected_memory_ids(event: &AgentEvent) -> Vec<String> {
    match event {
        AgentEvent::ContextManifest {
            injected_memory_ids,
            ..
        } => injected_memory_ids.clone(),
        other => panic!("expected ContextManifest, got {other:?}"),
    }
}

/// Extract `skill_names` from a `ContextManifest` event.
fn manifest_skill_names(event: &AgentEvent) -> Vec<String> {
    match event {
        AgentEvent::ContextManifest { skill_names, .. } => skill_names.clone(),
        other => panic!("expected ContextManifest, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Scenario 1 — Project instructions: CODEL00P.md / AGENTS.md / CLAUDE.md
// ---------------------------------------------------------------------------

/// Seeding `CODEL00P.md`, `AGENTS.md`, and `CLAUDE.md` in the workspace makes
/// the loader include all three in load order (CODEL00P.md first, then
/// AGENTS.md, then CLAUDE.md — as defined by `INSTRUCTION_FILES` in
/// `codel00p-harness/src/instructions.rs`).
///
/// Two assertions:
///
/// 1. `ContextManifest.instruction_sources` lists the files in that order.
/// 2. The first model request's `messages[0].content` (the system prompt)
///    contains all three files' instruction text.
#[test]
fn project_instructions_all_three_files_loaded_in_order() {
    let provider = MockProvider::start().assistant_text("done");

    let runner = CodelRunner::new()
        .workspace_file(
            "CODEL00P.md",
            "# Instructions\nUse snake_case for all identifiers.\n",
        )
        .workspace_file(
            "AGENTS.md",
            "# Agent notes\nAlways prefer idiomatic Rust.\n",
        )
        .workspace_file("CLAUDE.md", "# Claude notes\nKeep responses concise.\n")
        .with_provider(&provider);

    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    // 1. ContextManifest.instruction_sources lists files in defined load order.
    let manifest = result.assert_context_manifest();
    let sources = manifest_instruction_sources(manifest);
    assert_eq!(
        sources,
        vec!["CODEL00P.md", "AGENTS.md", "CLAUDE.md"],
        "instruction_sources should list all three files in INSTRUCTION_FILES order, got {sources:?}"
    );

    // 2. The first (and only) model request contains each file's instruction
    //    text embedded in the system prompt.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 1, "expected a single model round-trip");
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");

    let system_content = extract_system_content(&body);
    assert!(
        system_content.contains("Use snake_case for all identifiers"),
        "system prompt should contain CODEL00P.md text, got:\n{system_content}"
    );
    assert!(
        system_content.contains("Always prefer idiomatic Rust"),
        "system prompt should contain AGENTS.md text, got:\n{system_content}"
    );
    assert!(
        system_content.contains("Keep responses concise"),
        "system prompt should contain CLAUDE.md text, got:\n{system_content}"
    );
}

/// When only `CODEL00P.md` exists, only it appears in `instruction_sources`.
/// Files that are absent must not appear — the loader skips missing files.
#[test]
fn project_instructions_only_codel00p_md_when_others_absent() {
    let provider = MockProvider::start().assistant_text("done");

    let runner = CodelRunner::new()
        .workspace_file("CODEL00P.md", "# Instructions\nAlways write tests first.\n")
        // Deliberately NOT seeding AGENTS.md or CLAUDE.md.
        .with_provider(&provider);

    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    let manifest = result.assert_context_manifest();
    let sources = manifest_instruction_sources(manifest);
    assert_eq!(
        sources,
        vec!["CODEL00P.md"],
        "only CODEL00P.md was seeded; instruction_sources should list only that file, got {sources:?}"
    );

    // The instruction text must appear in the model request.
    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system_content = extract_system_content(&body);
    assert!(
        system_content.contains("Always write tests first"),
        "system prompt should contain CODEL00P.md content, got:\n{system_content}"
    );
}

/// An empty workspace (no instruction files) produces an empty
/// `instruction_sources` in the manifest — the loader skips blank/absent files.
#[test]
fn project_instructions_empty_when_no_instruction_files_present() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    let manifest = result.assert_context_manifest();
    let sources = manifest_instruction_sources(manifest);
    assert!(
        sources.is_empty(),
        "no instruction files seeded; instruction_sources should be empty, got {sources:?}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2 — Approved-memory injection
// ---------------------------------------------------------------------------

/// After a first `agent run` produces a `remember:` directive (which creates a
/// memory *candidate*), approving the candidate via `memory approve <id>` and
/// running a second `agent run` in the SAME `CODEL00P_HOME` injects the memory:
///
/// - `ContextManifest.injected_memory_ids` is non-empty.
/// - The second model request contains the memory content in the system prompt.
///
/// The two runs share a `CodelRunner` (same home + workspace) so the candidate
/// created in run 1 is visible to the approve command and to run 2.
#[test]
fn approved_memory_is_injected_into_second_run() {
    // --- Run 1: create a memory candidate via a `remember:` directive. ---
    let provider1 = MockProvider::start().assistant_text(
        "All done.\nremember convention[context-e2e]: prefer_small_functions over monoliths.",
    );
    let runner = CodelRunner::new().with_provider(&provider1);

    let result1 = runner.run(&["agent", "run", "Refactor the code.", "--tool-set", "all"]);
    result1.assert_success();

    // Confirm a candidate was persisted.
    let candidates = list_memory_with_status(&runner.memory_db(), MemoryStatus::Candidate);
    assert!(
        !candidates.is_empty(),
        "expected a memory candidate after the remember directive, got none"
    );

    // --- Approve each candidate via `memory approve <id>`. ---
    for candidate_id in &candidates {
        let approve = runner.run(&["memory", "approve", candidate_id]);
        assert!(
            approve.success(),
            "memory approve {candidate_id} should succeed\n--- stdout ---\n{}\n--- stderr ---\n{}",
            approve.stdout(),
            approve.stderr()
        );
    }

    // Confirm candidates now have Approved status.
    let approved = list_memory_with_status(&runner.memory_db(), MemoryStatus::Approved);
    assert!(
        !approved.is_empty(),
        "expected approved memories after running `memory approve`, got none"
    );

    // --- Run 2: same home/workspace, new mock provider. ---
    let provider2 = MockProvider::start().assistant_text("done");
    let runner = runner.with_provider(&provider2);

    let result2 = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result2.assert_success();

    // The manifest must report at least one injected memory id.
    let manifest = result2.assert_context_manifest();
    let injected = manifest_injected_memory_ids(manifest);
    assert!(
        !injected.is_empty(),
        "ContextManifest.injected_memory_ids should be non-empty after approving a memory, got {injected:?}"
    );

    // The second model request must contain the memory content.
    let requests2 = provider2.received_requests();
    assert_eq!(requests2.len(), 1, "expected a single round-trip in run 2");
    let body2: Value =
        serde_json::from_str(&requests2[0]).expect("run 2 request body should be valid JSON");
    let system2 = extract_system_content(&body2);
    assert!(
        system2.contains("prefer_small_functions"),
        "system prompt in run 2 should contain the approved memory content, got:\n{system2}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3 — Skill injection
// ---------------------------------------------------------------------------

/// Skills are loaded from `$CODEL00P_HOME/skills/<name>/SKILL.md`. We seed one
/// directly in the runner's home directory (no CLI involved — the skill is
/// authored by the test, not by the agent, which means it's treated as active
/// immediately without a review step). The trigger keyword matches the prompt
/// text, so the skill-selection logic selects and injects it.
///
/// Assertions:
/// - `ContextManifest.skill_names` contains the seeded skill's name.
/// - The first model request's system prompt contains the skill's body text.
#[test]
fn seeded_skill_matching_prompt_appears_in_context_manifest_and_system_prompt() {
    let skill_body = "# deploy-check skill\nAlways run `cargo test` before deploying.";
    let skill_content = format!(
        "---\nname: deploy-check\ndescription: Deployment checklist\ntriggers:\n  - deploy\n  - deployment\n---\n{skill_body}\n"
    );

    // Seed the skill under $CODEL00P_HOME/skills/deploy-check/SKILL.md.
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    // Write the skill file directly into the runner's home dir.
    let skill_dir = runner.home_path().join("skills").join("deploy-check");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(skill_dir.join("SKILL.md"), &skill_content).expect("write skill file");

    // The prompt contains the word "deploy" which matches the skill's triggers.
    let result = runner.run(&[
        "agent",
        "run",
        "Plan the deploy for today.",
        "--tool-set",
        "all",
    ]);
    result.assert_success();

    // 1. Manifest reports the skill was selected.
    let manifest = result.assert_context_manifest();
    let skill_names = manifest_skill_names(manifest);
    assert!(
        skill_names.iter().any(|n| n == "deploy-check"),
        "ContextManifest.skill_names should include 'deploy-check', got {skill_names:?}"
    );

    // 2. The model request's system prompt contains the skill's instruction body.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 1, "expected a single model round-trip");
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        system.contains("cargo test"),
        "system prompt should contain the skill body ('cargo test'), got:\n{system}"
    );
}

/// A skill whose trigger does NOT match the prompt text is NOT selected —
/// `skill_names` remains empty.
#[test]
fn skill_with_non_matching_trigger_is_not_injected() {
    let skill_content = "---\nname: deploy-check\ndescription: Deployment checklist\ntriggers:\n  - deploy\n  - deployment\n---\n# deploy-check\nAlways run cargo test.\n";

    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    // Seed the skill.
    let skill_dir = runner.home_path().join("skills").join("deploy-check");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(skill_dir.join("SKILL.md"), skill_content).expect("write skill file");

    // Prompt does NOT mention "deploy" or "deployment".
    let result = runner.run(&["agent", "run", "Say hello.", "--tool-set", "all"]);
    result.assert_success();

    let manifest = result.assert_context_manifest();
    let skill_names = manifest_skill_names(manifest);
    assert!(
        !skill_names.iter().any(|n| n == "deploy-check"),
        "deploy-check trigger did not match the prompt; skill_names should not include it, got {skill_names:?}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4 — ContextManifest determinism
// ---------------------------------------------------------------------------

/// Two identical `agent run`s (same workspace, same CODEL00P_HOME, same prompt,
/// same instruction files) must produce the same `content_hash`.
///
/// This verifies that the manifest hash computation is deterministic and that
/// nothing non-deterministic (random IDs, timestamps, etc.) leaks into the
/// hash inputs.
#[test]
fn context_manifest_hash_is_deterministic_across_identical_runs() {
    // Run 1.
    let provider1 = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new()
        .workspace_file("CODEL00P.md", "# Instructions\nWrite idiomatic code.\n")
        .with_provider(&provider1);

    let result1 = runner.run(&["agent", "run", "Do the task.", "--tool-set", "all"]);
    result1.assert_success();
    let hash1 = manifest_content_hash(result1.assert_context_manifest());

    // Run 2 — same home, same workspace, same prompt.
    let provider2 = MockProvider::start().assistant_text("done");
    let runner = runner.with_provider(&provider2);

    let result2 = runner.run(&["agent", "run", "Do the task.", "--tool-set", "all"]);
    result2.assert_success();
    let hash2 = manifest_content_hash(result2.assert_context_manifest());

    assert_eq!(
        hash1, hash2,
        "identical runs must produce the same ContextManifest content_hash"
    );
    // Sanity: a SHA-256 hex is 64 chars.
    assert_eq!(
        hash1.len(),
        64,
        "content_hash should be a 64-char SHA-256 hex"
    );
}

/// Adding a second instruction file (`AGENTS.md`) changes the
/// `content_hash`, because the hash is computed over the sorted source-file
/// names (among other manifest inputs). Changing the *set* of present
/// instruction files is therefore detectable via the hash.
///
/// Note: the hash covers instruction *source names* (e.g. `["CODEL00P.md"]`),
/// not the file contents. Two runs with identically-named files but different
/// content produce the same hash — the hash is a manifest digest, not a
/// content digest. This is the documented semantic from `compute_manifest_hash`
/// in `codel00p-protocol/src/events.rs`.
#[test]
fn context_manifest_hash_changes_when_instruction_sources_change() {
    // Run A: only CODEL00P.md present.
    let provider_a = MockProvider::start().assistant_text("done");
    let runner_a = CodelRunner::new()
        .workspace_file("CODEL00P.md", "# Instructions\nWrite good code.\n")
        .with_provider(&provider_a);
    let result_a = runner_a.run(&["agent", "run", "Do the task.", "--tool-set", "all"]);
    result_a.assert_success();
    let manifest_a = result_a.assert_context_manifest();
    let hash_a = manifest_content_hash(manifest_a);
    let sources_a = manifest_instruction_sources(manifest_a);
    assert_eq!(sources_a, vec!["CODEL00P.md"]);

    // Run B: both CODEL00P.md and AGENTS.md present — a different source set.
    let provider_b = MockProvider::start().assistant_text("done");
    let runner_b = CodelRunner::new()
        .workspace_file("CODEL00P.md", "# Instructions\nWrite good code.\n")
        .workspace_file("AGENTS.md", "# Agent notes\nBe idiomatic.\n")
        .with_provider(&provider_b);
    let result_b = runner_b.run(&["agent", "run", "Do the task.", "--tool-set", "all"]);
    result_b.assert_success();
    let manifest_b = result_b.assert_context_manifest();
    let hash_b = manifest_content_hash(manifest_b);
    let sources_b = manifest_instruction_sources(manifest_b);
    assert_eq!(sources_b, vec!["CODEL00P.md", "AGENTS.md"]);

    assert_ne!(
        hash_a, hash_b,
        "adding a second instruction file must produce a different content_hash \
         (hash covers source-file names, not content)"
    );
}

// ---------------------------------------------------------------------------
// Scenario 6 — Agent self-awareness (self/capability block injection)
// ---------------------------------------------------------------------------

/// By default (`agent.behavior.self_knowledge` unset = on), a plain `agent run`
/// injects a self block whose first line identifies the agent. We assert the
/// identity line ("You are codel00p v…") is present in the system prompt the
/// model was shown, along with a capabilities line.
#[test]
fn default_run_injects_self_identity_block() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 1, "expected a single model round-trip");
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        system.contains("You are codel00p v"),
        "default run should inject the self identity line, got:\n{system}"
    );
    assert!(
        system.contains("Capabilities:"),
        "default run should inject the capabilities line, got:\n{system}"
    );
}

/// Setting `agent.behavior.self_knowledge=false` drops the identity/capabilities
/// block — the system prompt the model sees no longer contains the identity line.
#[test]
fn self_knowledge_off_omits_self_identity_block() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    // Disable the self-knowledge facet via the real config surface.
    let set = runner.run(&["config", "set", "agent.behavior.self_knowledge", "false"]);
    assert!(
        set.success(),
        "config set should succeed\n--- stdout ---\n{}\n--- stderr ---\n{}",
        set.stdout(),
        set.stderr()
    );

    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        !system.contains("You are codel00p v"),
        "self_knowledge=false should omit the identity line, got:\n{system}"
    );
    assert!(
        !system.contains("Capabilities:"),
        "self_knowledge=false should omit the capabilities line, got:\n{system}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 7 — Base operating prompt ("how I work") injection
// ---------------------------------------------------------------------------

/// By default (`agent.behavior.base_prompt` unset = on), a plain `agent run`
/// injects the base operating prompt. We assert its rigor guidance and the
/// planning guidance (auto_plan defaults on) appear in the system prompt.
#[test]
fn default_run_injects_base_operating_prompt() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    let result = runner.run(&["agent", "run", "Do work.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        system.contains("Verify before you declare done"),
        "default run should inject the base prompt rigor guidance, got:\n{system}"
    );
    assert!(
        system.contains("lay out a short plan"),
        "default run (auto_plan on) should include planning guidance, got:\n{system}"
    );
}

/// Setting `agent.behavior.base_prompt=false` drops the base block entirely —
/// the system prompt no longer contains its rigor guidance.
#[test]
fn base_prompt_off_omits_base_operating_prompt() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    let set = runner.run(&["config", "set", "agent.behavior.base_prompt", "false"]);
    assert!(
        set.success(),
        "config set should succeed\n--- stdout ---\n{}\n--- stderr ---\n{}",
        set.stdout(),
        set.stderr()
    );

    let result = runner.run(&["agent", "run", "Do work.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        !system.contains("Verify before you declare done"),
        "base_prompt=false should omit the base operating prompt, got:\n{system}"
    );
}

/// Setting `agent.behavior.auto_plan=false` keeps the base prompt but drops the
/// planning guidance, so a minimal profile stays quieter.
#[test]
fn auto_plan_off_keeps_base_prompt_but_drops_planning_guidance() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);

    let set = runner.run(&["config", "set", "agent.behavior.auto_plan", "false"]);
    assert!(set.success(), "config set should succeed: {}", set.stderr());

    let result = runner.run(&["agent", "run", "Do work.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        system.contains("Verify before you declare done"),
        "auto_plan=false should keep the base prompt core, got:\n{system}"
    );
    assert!(
        !system.contains("lay out a short plan"),
        "auto_plan=false should drop planning guidance, got:\n{system}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 8 — Workspace / build-test awareness ("Workspace state" block)
// ---------------------------------------------------------------------------

/// By default (`agent.behavior.workspace_context` unset = on), a plain
/// `agent run` injects a "Workspace state" block. With a `Cargo.toml` in the
/// workspace, the detected test/build/lint commands appear so the model knows how
/// to verify without guessing.
#[test]
fn default_run_injects_workspace_state_block_with_detected_commands() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new()
        .workspace_file(
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .with_provider(&provider);

    let result = runner.run(&["agent", "run", "Do work.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        system.contains("Workspace state"),
        "default run should inject the workspace-state block, got:\n{system}"
    );
    assert!(
        system.contains("test = `cargo test`"),
        "workspace-state block should list the detected test command, got:\n{system}"
    );
    assert!(
        system.contains("from Cargo.toml"),
        "workspace-state block should name the detection source, got:\n{system}"
    );
}

/// Setting `agent.behavior.workspace_context=false` drops the workspace-state
/// block entirely — the system prompt no longer contains it.
#[test]
fn workspace_context_off_omits_workspace_state_block() {
    let provider = MockProvider::start().assistant_text("done");
    let runner = CodelRunner::new()
        .workspace_file(
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .with_provider(&provider);

    let set = runner.run(&["config", "set", "agent.behavior.workspace_context", "false"]);
    assert!(
        set.success(),
        "config set should succeed\n--- stdout ---\n{}\n--- stderr ---\n{}",
        set.stdout(),
        set.stderr()
    );

    let result = runner.run(&["agent", "run", "Do work.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let system = extract_system_content(&body);
    assert!(
        !system.contains("Workspace state"),
        "workspace_context=false should omit the workspace-state block, got:\n{system}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5 — Context compaction (documented as not feasible headlessly)
// ---------------------------------------------------------------------------
//
// Context compaction (`ContextCompacted` event) fires when the accumulated
// session message count exceeds the compaction threshold. That threshold is
// configured on `AgentHarness::builder()` and has no corresponding
// `agent run` CLI flag. The default threshold is large enough that a single
// headless turn with one model round-trip never reaches it. Triggering
// compaction headlessly would require either:
//
//   a) Many consecutive `agent continue` turns (not scriptable with the
//      current `CodelRunner` API which targets `agent run`), or
//   b) A `--compaction-threshold` flag (does not exist in the CLI).
//
// Compaction is therefore exercised at the harness unit-test layer
// (`codel00p-harness/tests/`) where the builder threshold can be set
// programmatically. This block is left as documentation only.

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract the concatenated text of every `system`-role message from a
/// chat-completions request body.
///
/// The system prompt may be a single `{"role":"system","content":"..."}` entry
/// in the `messages` array, or the content may be a JSON array of text blocks.
/// We handle both shapes and fall back to the raw body for any unparseable form.
fn extract_system_content(body: &Value) -> String {
    let messages = match body.get("messages").and_then(Value::as_array) {
        Some(arr) => arr,
        None => return body.to_string(),
    };

    let mut parts = Vec::new();
    for msg in messages {
        if msg.get("role").and_then(Value::as_str) != Some("system") {
            continue;
        }
        match msg.get("content") {
            Some(Value::String(text)) => parts.push(text.clone()),
            Some(Value::Array(blocks)) => {
                for block in blocks {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        parts.push(text.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        // Fallback: search the whole body so we don't silently skip.
        body.to_string()
    } else {
        parts.join("\n")
    }
}

// Silence the unused-import lint: `json!` is used in some test helpers above
// even if the compiler can't see it from a single scan.
const _: Value = json!(null);
