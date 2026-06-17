//! Memory loop end-to-end tests.
//!
//! Proves the cross-feature MEMORY LOOP: extract → review → inject. Each
//! scenario is hermetic: a single `CodelRunner` owns one `CODEL00P_HOME` and
//! one workspace, so all three binary invocations in a scenario share the same
//! `memory.sqlite`.
//!
//! Per-command memory coverage lives in `codel00p-cli/tests/memory_cli/`; these
//! tests focus exclusively on the **end-to-end loop** through the real binary +
//! mock provider.
//!
//! # Post-session recommendation (PR #44)
//!
//! The `DeterministicMemoryRecommender` is defined in `codel00p-harness` and
//! the builder exposes a `memory_recommender()` setter, but the CLI's
//! `build_agent_harness_with` (in `codel00p-cli/src/agent/turn.rs`) does NOT
//! call `.memory_recommender(...)`. The recommender is therefore **not wired in
//! the CLI path** at this revision. The corresponding scenario below is
//! documented and marked `#[ignore]` so the suite stays green, and the comment
//! describes exactly what to wire up when the CLI wires the recommender.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use codel00p_memory::{MemoryListFilter, MemoryRepository, StorageBackedMemoryStore};
use codel00p_protocol::{MemoryStatus, ProjectRef};
use codel00p_storage::{SqliteStorage, StorageScope};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Opens the memory store in the runner's `CODEL00P_HOME` and returns all
/// candidate IDs and content strings (status == Candidate).
fn list_candidates(runner: &CodelRunner) -> Vec<(String, String)> {
    let storage = SqliteStorage::open(runner.memory_db()).expect("open memory.sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let project = ProjectRef::new("project-1", "codel00p");
    let filter = MemoryListFilter::new(project).with_status(MemoryStatus::Candidate);
    store
        .list(filter)
        .expect("list memory candidates")
        .into_iter()
        .map(|rec| {
            (
                rec.entry().id().to_string(),
                rec.entry().content().to_string(),
            )
        })
        .collect()
}

/// Parses `memory list --json` stdout into a `Vec<Value>`.
fn parse_memory_list_json(stdout: &str) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(stdout.trim())
        .expect("memory list --json should be valid JSON")
}

/// Parses `memory audit <id> --json` stdout into a `Vec<Value>`.
fn parse_audit_json(stdout: &str) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(stdout.trim())
        .expect("memory audit --json should be valid JSON")
}

// ---------------------------------------------------------------------------
// Scenario 1: Extract → Persist
//
// An `agent run` whose final assistant text contains a `remember:` directive
// causes a memory candidate to be persisted and appear in `memory list --json`
// with status "candidate".
// ---------------------------------------------------------------------------

#[test]
fn memory_extract_and_persist_candidate() {
    let runner = CodelRunner::new();

    // Script: single assistant turn with an explicit `remember:` directive.
    let provider = MockProvider::start()
        .assistant_text("Done.\nremember convention[loop-test]: the loop test convention.");

    let runner = runner.with_provider(&provider);

    let result = runner.run(&["agent", "run", "Register the loop convention."]);
    result.assert_success();

    // 1a. The in-process memory store has the candidate.
    let candidates = list_candidates(&runner);
    assert!(
        candidates
            .iter()
            .any(|(_, content)| content.contains("loop test convention")),
        "expected a persisted candidate from remember: directive, got: {candidates:?}"
    );

    // 1b. `memory list --json --status candidate` also shows it via the real binary.
    let list_result = runner.run(&["memory", "list", "--json", "--status", "candidate"]);
    list_result.assert_success();
    let items = parse_memory_list_json(list_result.stdout());
    assert!(
        items.iter().any(|item| item["content"]
            .as_str()
            .unwrap_or("")
            .contains("loop test convention")),
        "memory list --json should contain the extracted candidate, got: {items:?}"
    );
    // Every item in the list must be a candidate.
    for item in &items {
        assert_eq!(
            item["status"].as_str().unwrap_or(""),
            "candidate",
            "unexpected status in memory list output: {item}"
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 2: Approve → Inject
//
// After approving a candidate, a SECOND `agent run` must inject the approved
// memory into the model request. The `ContextManifest` event should list the
// approved ID in `injected_memory_ids`, and the raw model request body must
// contain the memory content.
// ---------------------------------------------------------------------------

#[test]
fn memory_approve_then_inject_into_second_run() {
    let runner = CodelRunner::new();

    // --- Turn 1: extract a candidate ---
    let provider1 = MockProvider::start()
        .assistant_text("Done.\nremember convention[approve-inject]: the approved fact content.");
    let runner = runner.with_provider(&provider1);
    let run1 = runner.run(&["agent", "run", "Record the approved fact."]);
    run1.assert_success();

    // Retrieve the candidate id from the persisted store.
    let candidates = list_candidates(&runner);
    let (candidate_id, _) = candidates
        .iter()
        .find(|(_, content)| content.contains("approved fact content"))
        .cloned()
        .expect("candidate from remember directive must be persisted");

    // --- Approve via the real binary ---
    let approve = runner.run(&["memory", "approve", &candidate_id]);
    approve.assert_success();

    // `memory show <id>` should now report "approved".
    let show = runner.run(&["memory", "show", &candidate_id, "--json"]);
    show.assert_success();
    let record: Value =
        serde_json::from_str(show.stdout().trim()).expect("memory show --json must be valid JSON");
    assert_eq!(
        record["status"].as_str().unwrap_or(""),
        "approved",
        "memory status should be 'approved' after approve command, got: {record}"
    );

    // --- Turn 2: a fresh agent run should inject the approved memory ---
    let provider2 = MockProvider::start().assistant_text("Done with second run.");
    // Reattach a new provider; the runner keeps the same CODEL00P_HOME.
    let runner = runner.with_provider(&provider2);
    let run2 = runner.run(&["agent", "run", "Do a second task."]);
    run2.assert_success();

    // 2a. ContextManifest must list the approved id in injected_memory_ids.
    let manifest = run2.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        injected_memory_ids,
        ..
    } = manifest
    {
        assert!(
            injected_memory_ids.contains(&candidate_id),
            "injected_memory_ids should contain {candidate_id}, got: {injected_memory_ids:?}"
        );
    } else {
        panic!("assert_context_manifest returned a non-ContextManifest event");
    }

    // 2b. The raw request body the model received must contain the memory content.
    let requests = provider2.received_requests();
    assert!(
        !requests.is_empty(),
        "provider2 must have received at least one request"
    );
    let first_request = &requests[0];
    assert!(
        first_request.contains("approved fact content"),
        "model request should contain approved memory content, got:\n{first_request}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: Reject → Not Injected
//
// A rejected candidate must NOT appear in subsequent runs.
// ---------------------------------------------------------------------------

#[test]
fn memory_reject_then_not_injected() {
    let runner = CodelRunner::new();

    // --- Turn 1: extract a candidate ---
    let provider1 = MockProvider::start()
        .assistant_text("Done.\nremember convention[reject-test]: the rejected fact content.");
    let runner = runner.with_provider(&provider1);
    let run1 = runner.run(&["agent", "run", "Record the rejected fact."]);
    run1.assert_success();

    let candidates = list_candidates(&runner);
    let (candidate_id, _) = candidates
        .iter()
        .find(|(_, content)| content.contains("rejected fact content"))
        .cloned()
        .expect("candidate must be persisted");

    // --- Reject via the real binary ---
    let reject = runner.run(&[
        "memory",
        "reject",
        &candidate_id,
        "--reason",
        "not relevant for e2e test",
    ]);
    reject.assert_success();

    // Confirm rejection.
    let show = runner.run(&["memory", "show", &candidate_id, "--json"]);
    show.assert_success();
    let record: Value =
        serde_json::from_str(show.stdout().trim()).expect("memory show --json must be valid JSON");
    assert_eq!(
        record["status"].as_str().unwrap_or(""),
        "rejected",
        "memory status should be 'rejected' after reject command, got: {record}"
    );

    // --- Turn 2: the rejected memory must NOT be injected ---
    let provider2 = MockProvider::start().assistant_text("Done with second run.");
    let runner = runner.with_provider(&provider2);
    let run2 = runner.run(&["agent", "run", "Do a second task."]);
    run2.assert_success();

    // 3a. ContextManifest must NOT include the rejected id.
    let manifest = run2.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        injected_memory_ids,
        ..
    } = manifest
    {
        assert!(
            !injected_memory_ids.contains(&candidate_id),
            "rejected memory id {candidate_id} must not appear in injected_memory_ids, got: {injected_memory_ids:?}"
        );
    } else {
        panic!("assert_context_manifest returned a non-ContextManifest event");
    }

    // 3b. The raw request body must NOT contain the rejected memory content.
    let requests = provider2.received_requests();
    assert!(
        !requests.is_empty(),
        "provider2 must have received at least one request"
    );
    let first_request = &requests[0];
    assert!(
        !first_request.contains("rejected fact content"),
        "model request should NOT contain rejected memory content, got:\n{first_request}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: Audit trail
//
// `memory audit <id> --json` reflects the candidate_created action and the
// approve or reject action applied in scenarios 2 and 3.
// ---------------------------------------------------------------------------

#[test]
fn memory_audit_trail_reflects_review_actions() {
    let runner = CodelRunner::new();

    // Extract one candidate to approve and one to reject.
    let provider = MockProvider::start().assistant_text(
        "Done.\n\
         remember convention[audit-approve]: fact to be approved.\n\
         remember convention[audit-reject]: fact to be rejected.",
    );
    let runner = runner.with_provider(&provider);
    let run1 = runner.run(&["agent", "run", "Record two audit facts."]);
    run1.assert_success();

    let candidates = list_candidates(&runner);

    let (approve_id, _) = candidates
        .iter()
        .find(|(_, content)| content.contains("fact to be approved"))
        .cloned()
        .expect("approve candidate must be persisted");

    let (reject_id, _) = candidates
        .iter()
        .find(|(_, content)| content.contains("fact to be rejected"))
        .cloned()
        .expect("reject candidate must be persisted");

    // Apply approve and reject.
    runner
        .run(&["memory", "approve", &approve_id])
        .assert_success();
    runner
        .run(&[
            "memory",
            "reject",
            &reject_id,
            "--reason",
            "audit trail test",
        ])
        .assert_success();

    // Audit the approved entry: must have candidate_created + approved entries.
    let audit_approve = runner.run(&["memory", "audit", &approve_id, "--json"]);
    audit_approve.assert_success();
    let approve_events = parse_audit_json(audit_approve.stdout());
    assert!(
        approve_events
            .iter()
            .any(|ev| ev["action"].as_str() == Some("candidate_created")),
        "audit for approved entry must contain 'candidate_created', got: {approve_events:?}"
    );
    assert!(
        approve_events
            .iter()
            .any(|ev| ev["action"].as_str() == Some("approved")),
        "audit for approved entry must contain 'approved' action, got: {approve_events:?}"
    );

    // Audit the rejected entry: must have candidate_created + rejected entries.
    let audit_reject = runner.run(&["memory", "audit", &reject_id, "--json"]);
    audit_reject.assert_success();
    let reject_events = parse_audit_json(audit_reject.stdout());
    assert!(
        reject_events
            .iter()
            .any(|ev| ev["action"].as_str() == Some("candidate_created")),
        "audit for rejected entry must contain 'candidate_created', got: {reject_events:?}"
    );
    assert!(
        reject_events
            .iter()
            .any(|ev| ev["action"].as_str() == Some("rejected")),
        "audit for rejected entry must contain 'rejected' action, got: {reject_events:?}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5: Post-session recommendation (PR #44) — NOT YET WIRED IN CLI
//
// The `DeterministicMemoryRecommender` lives in `codel00p-harness` and is
// accessible via `AgentHarness::builder().memory_recommender(...)`. The CLI's
// `build_agent_harness_with` (codel00p-cli/src/agent/turn.rs) currently does
// NOT call `.memory_recommender(...)`, so this scenario is `#[ignore]`d.
//
// To wire it: in `build_agent_harness_with`, after the existing
//   `.turn_memory_extractor(memory_extractor)`
// line, add:
//   .memory_recommender(Arc::new(
//       DeterministicMemoryRecommender::new(config.project.clone())
//           .with_tag("agent")
//           .with_tag("cli"),
//   ))
//
// Once wired, remove the `#[ignore]` below and the test should pass as-is:
// after an `agent run` with at least one mutating tool call and a final answer,
// `memory list --json --status candidate` should contain an entry tagged
// `auto-recommended` even without any explicit `remember:` directive.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "DeterministicMemoryRecommender not yet wired in CLI build_agent_harness_with (codel00p-cli/src/agent/turn.rs)"]
fn memory_post_session_recommendation_without_explicit_directive() {
    let runner = CodelRunner::new()
        .workspace_file("seed.txt", "seed")
        .git_init();

    // A turn that uses a mutating tool but emits NO `remember:` directive.
    let provider = MockProvider::start()
        .tool_call("create_file", json!({"path": "rec.txt", "content": "hi"}))
        .assistant_text("File created. No explicit remember directive here.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Create rec.txt.", "--tool-set", "all"]);
    result.assert_success();

    // The DeterministicMemoryRecommender should have produced a candidate
    // tagged `auto-recommended`.
    let list = runner.run(&["memory", "list", "--json", "--status", "candidate"]);
    list.assert_success();
    let items = parse_memory_list_json(list.stdout());
    let has_auto_recommended = items.iter().any(|item| {
        item["tags"]
            .as_array()
            .map(|tags| tags.iter().any(|t| t.as_str() == Some("auto-recommended")))
            .unwrap_or(false)
    });
    assert!(
        has_auto_recommended,
        "expected an auto-recommended candidate after a mutating turn, got: {items:?}"
    );
}
