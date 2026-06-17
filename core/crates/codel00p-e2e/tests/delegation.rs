//! End-to-end scenarios for sub-agent delegation, driving the **real**
//! `codel00p` binary through the `--tool-set delegate` path against a scripted
//! mock provider.
//!
//! # What the CLI actually wires (verified, not assumed)
//!
//! `agent run --tool-set delegate` folds in the `delegate_task` tool backed by
//! `CliSubAgentSpawner` (see `codel00p-cli/src/agent/{turn,delegation}.rs`). Two
//! facts about that wiring shape these tests:
//!
//! 1. **The child uses the SAME provider as the parent.** The spawner builds the
//!    child's `ProviderModelClient` from the parent's `provider` / `model` /
//!    `base_url` — i.e. our mock server. So a delegation makes the parent harness
//!    AND the child harness both hit the one mock, each with its own
//!    conversation. The mock therefore scripts parent and child turns separately,
//!    scoped by [`codel00p_e2e::Audience`]: the orchestrator parent is the only
//!    side that advertises `delegate_task`, so `parent_*` turns fire only on its
//!    requests and `child_*` turns only on the child's. Without that scoping both
//!    conversations start at zero tool results and their turn indices collide.
//!
//! 2. **The CLI child is read-only and isolation-agnostic.** `CliSubAgentSpawner`
//!    hard-codes the child's tool set to `ToolRegistry::read_only_defaults()` and
//!    never inspects `task.isolation()` — it always runs the child against the
//!    parent workspace and never captures a diff. So through the CLI:
//!      * a delegated child can read parent-workspace files but cannot create
//!        them, and
//!      * passing `isolation: "worktree"` is accepted but is a no-op (no diff,
//!        shared workspace).
//!
//! The *real* worktree-isolation behavior the prompt describes — a mutating child
//! in its own throwaway worktree, the captured diff, the parent tree left
//! untouched — lives one layer down, on `HarnessSubAgentSpawner`, and is already
//! covered end-to-end in `codel00p-harness/tests/worktree_isolation.rs`
//! (`worktree_isolation_mutates_in_a_worktree_and_returns_diff`,
//! `delegate_tool_surfaces_diff_for_worktree_isolation`). These CLI-level tests
//! deliberately assert what the CLI *does* expose and document the boundary, per
//! the task rules, rather than re-testing the harness mechanism.
//!
//! Every test is fully hermetic: its own `CODEL00P_HOME`, workspace tempdir, and
//! `memory.sqlite`. No network, no API keys.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::{Value, json};

/// Returns the `content` of the `delegate_task` tool-result message the parent
/// was shown on a later round-trip, parsed as JSON. This is the child's outcome
/// (`summary` / `child_session_id` / `tool_calls`, and a `diff` only under real
/// worktree isolation) flowing back into the parent's conversation — the seam
/// where "the child's summary surfaces" is observable hermetically.
fn delegate_result(provider: &MockProvider) -> Value {
    for body in provider.received_requests() {
        let request: Value = match serde_json::from_str(&body) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(messages) = request.get("messages").and_then(Value::as_array) else {
            continue;
        };
        for message in messages {
            let is_tool = message.get("role").and_then(Value::as_str) == Some("tool");
            let is_delegate = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.contains("delegate_task"));
            if is_tool
                && is_delegate
                && let Some(content) = message.get("content").and_then(Value::as_str)
                && let Ok(parsed) = serde_json::from_str::<Value>(content)
            {
                return parsed;
            }
        }
    }
    panic!(
        "no delegate_task tool result was fed back to the parent.\nrequests: {:#?}",
        provider.received_requests()
    );
}

// ---------------------------------------------------------------------------
// Shared isolation (the CLI default and only mode).
//
// The orchestrator delegates a focused task; the child runs as its own harness
// turn against the SAME mock provider and the SAME (parent) workspace, reads a
// seeded file, and reports a summary. We assert the full round trip: the
// delegate tool ran, the child's summary surfaced back into the parent's
// conversation, and the child genuinely operated on the parent workspace (its
// read of `data.txt` returned the parent's seeded content).
// ---------------------------------------------------------------------------

#[test]
fn shared_delegation_runs_child_against_parent_workspace_and_surfaces_summary() {
    // Seed a file the child will read from the *parent* workspace.
    let runner = CodelRunner::new().workspace_file("data.txt", "shared-workspace-content\n");

    // Parent and child turns are scoped so their indices don't collide (see the
    // module docs): the parent delegates, the child reads + summarizes, the
    // parent emits its final text.
    let provider = MockProvider::start()
        // Parent turn 0: hand a read task to a child.
        .parent_tool_call(
            "delegate_task",
            json!({ "task": "read data.txt and report its content", "label": "reader" }),
        )
        // Child turn 0: read the seeded file (in the shared parent workspace).
        .child_tool_call("read_file", json!({ "path": "data.txt" }))
        // Child turn 1: report a summary. Carries a marker we can find in the
        // delegate result the parent is later shown.
        .child_text("CHILD-SUMMARY: data.txt holds shared-workspace-content")
        // Parent final turn (after the delegate result comes back).
        .parent_text("Delegation complete.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Delegate reading data.txt to a sub-agent.",
        "--tool-set",
        "delegate",
        "--tool-set",
        "read",
    ]);
    result.assert_success();

    // The orchestrator requested AND completed the delegation.
    result.assert_tool_called("delegate_task");
    result.assert_turn_completed();

    // Exactly the four scripted round-trips ran (one parent delegate, two child
    // turns, one parent final) — no phantom/duplicated turns from index
    // collisions, which is what the audience scoping guarantees.
    assert_eq!(
        provider.hits(),
        4,
        "expected 4 model round-trips (parent delegate, child read, child summary, parent final)"
    );

    // The child's summary surfaces back into the parent's conversation via the
    // delegate_task tool result.
    let outcome = delegate_result(&provider);
    assert_eq!(
        outcome["summary"].as_str(),
        Some("CHILD-SUMMARY: data.txt holds shared-workspace-content"),
        "the child's summary should surface in the delegate result, got: {outcome}"
    );
    // The child ran its own tool, recorded on the outcome.
    assert_eq!(
        outcome["tool_calls"].as_u64(),
        Some(1),
        "the child ran one tool (read_file), got: {outcome}"
    );
    // The child is its own session linked under the orchestrator.
    assert!(
        outcome["child_session_id"].as_str().is_some(),
        "the delegate result should carry the child's session id, got: {outcome}"
    );

    // Shared isolation: the CLI never captures a diff (that is a worktree-only
    // concern, and the CLI does not do worktree isolation).
    assert!(
        outcome.get("diff").is_none(),
        "shared delegation must not produce a diff, got: {outcome}"
    );

    // Proof the child operated on the PARENT workspace (shared isolation): its
    // read_file returned the parent's seeded content, which the child then
    // referenced in its summary. The shared workspace is what made that read
    // succeed — a child with its own isolated tree would not have seen data.txt.
    let child_saw_seeded_content = provider
        .received_requests()
        .iter()
        .any(|body| body.contains("shared-workspace-content") && body.contains("read_file"));
    assert!(
        child_saw_seeded_content,
        "the child should have read the parent workspace's seeded data.txt content"
    );
}

// ---------------------------------------------------------------------------
// Worktree isolation requested through the CLI.
//
// The delegate tool accepts `isolation: "worktree"`, but the CLI spawner ignores
// it: the child still runs shared and NO diff is captured. This test pins that
// actual CLI behavior. The genuine worktree mechanism (own worktree, captured
// diff, parent tree untouched) is exercised at the harness layer in
// codel00p-harness/tests/worktree_isolation.rs — see the module docs.
// ---------------------------------------------------------------------------

#[test]
fn worktree_isolation_arg_is_accepted_but_the_cli_runs_shared_with_no_diff() {
    // A real git repo, so that IF the CLI honored worktree isolation it could —
    // making the "no diff / shared" assertions meaningful rather than incidental.
    let runner = CodelRunner::new()
        .workspace_file("data.txt", "parent-tree-content\n")
        .git_init();

    let provider = MockProvider::start()
        // Parent delegates WITH worktree isolation requested.
        .parent_tool_call(
            "delegate_task",
            json!({
                "task": "read data.txt",
                "label": "reader",
                "isolation": "worktree"
            }),
        )
        // Child reads the file and summarizes.
        .child_tool_call("read_file", json!({ "path": "data.txt" }))
        .child_text("CHILD-SUMMARY: read data.txt")
        .parent_text("Delegation complete.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Delegate with worktree isolation.",
        "--tool-set",
        "delegate",
        "--tool-set",
        "read",
    ]);
    // The run succeeds: the isolation arg is accepted (the tool's schema allows
    // it), the delegation completes.
    result.assert_success();
    result.assert_tool_called("delegate_task");
    result.assert_turn_completed();

    let outcome = delegate_result(&provider);
    assert_eq!(
        outcome["summary"].as_str(),
        Some("CHILD-SUMMARY: read data.txt"),
        "the child's summary still surfaces, got: {outcome}"
    );

    // What the CLI exposes: NO diff is captured for worktree isolation, because
    // CliSubAgentSpawner ignores task.isolation() and runs the child shared.
    assert!(
        outcome.get("diff").is_none(),
        "the CLI does not implement worktree isolation, so no diff is surfaced; got: {outcome}"
    );

    // And because it ran shared, the child saw the parent workspace's file (its
    // read returned the parent's content) rather than an isolated empty tree.
    let child_saw_parent_content = provider
        .received_requests()
        .iter()
        .any(|body| body.contains("parent-tree-content") && body.contains("read_file"));
    assert!(
        child_saw_parent_content,
        "the worktree arg is a no-op in the CLI: the child ran shared and read the parent file"
    );
}

// ---------------------------------------------------------------------------
// The delegate tool is gated behind the `delegate` tool set.
//
// Without `--tool-set delegate`, the orchestrator has no `delegate_task` tool, so
// it cannot spawn children — the bound on uncontrolled delegation. We confirm the
// tool is advertised only when the set is enabled.
// ---------------------------------------------------------------------------

#[test]
fn delegate_task_is_only_advertised_with_the_delegate_tool_set() {
    // With the delegate set: the context manifest advertises delegate_task.
    let with_delegate = MockProvider::start().assistant_text("nothing to do");
    let runner = CodelRunner::new().with_provider(&with_delegate);
    let result = runner.run(&[
        "agent",
        "run",
        "hello",
        "--tool-set",
        "delegate",
        "--tool-set",
        "read",
    ]);
    result.assert_success();
    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        assert!(
            advertised_tools.iter().any(|tool| tool == "delegate_task"),
            "delegate_task should be advertised with --tool-set delegate, got {advertised_tools:?}"
        );
    }

    // Without it (read-only): delegate_task is NOT advertised.
    let without_delegate = MockProvider::start().assistant_text("nothing to do");
    let runner = CodelRunner::new().with_provider(&without_delegate);
    let result = runner.run(&["agent", "run", "hello", "--tool-set", "read"]);
    result.assert_success();
    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        assert!(
            !advertised_tools.iter().any(|tool| tool == "delegate_task"),
            "delegate_task must not be advertised without the delegate set, got {advertised_tools:?}"
        );
    }
}
