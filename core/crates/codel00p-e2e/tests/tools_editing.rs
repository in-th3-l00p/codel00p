//! End-to-end scenarios for the editing tool set.
//!
//! Exercises `create_file`, `update_file`, `delete_file`, and `apply_patch`
//! (single-file, multi-file atomic, and atomicity-failure cases) against the
//! real `codel00p` binary with a scripted mock provider.
//!
//! Every test is fully hermetic: its own `CODEL00P_HOME`, workspace tempdir,
//! and `memory.sqlite`. No network, no API keys.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

// ---------------------------------------------------------------------------
// create_file
// ---------------------------------------------------------------------------

#[test]
fn create_file_creates_file_with_expected_contents() {
    let runner = CodelRunner::new();
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "hello.txt", "content": "hello world\n" }),
        )
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Create hello.txt.", "--tool-set", "edit"]);
    result.assert_success();
    result.assert_tool_called("create_file");
    result.assert_turn_completed();

    let file_path = runner.workspace_path().join("hello.txt");
    assert!(file_path.exists(), "hello.txt should have been created");
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "hello world\n",
        "hello.txt contents should match"
    );
}

// ---------------------------------------------------------------------------
// update_file
// ---------------------------------------------------------------------------

#[test]
fn update_file_replaces_existing_file_contents() {
    let runner = CodelRunner::new().workspace_file("notes.txt", "original content\n");

    let provider = MockProvider::start()
        .tool_call(
            "update_file",
            json!({ "path": "notes.txt", "content": "updated content\n" }),
        )
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Update notes.txt.", "--tool-set", "edit"]);
    result.assert_success();
    result.assert_tool_called("update_file");
    result.assert_turn_completed();

    let file_path = runner.workspace_path().join("notes.txt");
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "updated content\n",
        "notes.txt should have been replaced with new contents"
    );
}

// ---------------------------------------------------------------------------
// delete_file
// ---------------------------------------------------------------------------

#[test]
fn delete_file_removes_seeded_file() {
    let runner = CodelRunner::new().workspace_file("to_delete.txt", "temporary\n");

    let provider = MockProvider::start()
        .tool_call("delete_file", json!({ "path": "to_delete.txt" }))
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Delete to_delete.txt.",
        "--tool-set",
        "edit",
    ]);
    result.assert_success();
    result.assert_tool_called("delete_file");
    result.assert_turn_completed();

    let file_path = runner.workspace_path().join("to_delete.txt");
    assert!(
        !file_path.exists(),
        "to_delete.txt should have been deleted"
    );
}

// ---------------------------------------------------------------------------
// apply_patch — single-file edit
// ---------------------------------------------------------------------------

#[test]
fn apply_patch_single_file_changes_targeted_substring() {
    let runner =
        CodelRunner::new().workspace_file("config.toml", "[settings]\nversion = \"1.0\"\n");

    let provider = MockProvider::start()
        .tool_call(
            "apply_patch",
            json!({
                "changes": [{
                    "path": "config.toml",
                    "find": "version = \"1.0\"",
                    "replace": "version = \"2.0\""
                }]
            }),
        )
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Bump config version.", "--tool-set", "edit"]);
    result.assert_success();
    result.assert_tool_called("apply_patch");
    result.assert_turn_completed();

    let contents = std::fs::read_to_string(runner.workspace_path().join("config.toml")).unwrap();
    assert!(
        contents.contains("version = \"2.0\""),
        "config.toml should contain the updated version string, got: {contents:?}"
    );
    assert!(
        !contents.contains("version = \"1.0\""),
        "config.toml should no longer contain the old version string, got: {contents:?}"
    );
}

// ---------------------------------------------------------------------------
// apply_patch — multi-file atomic edit
// ---------------------------------------------------------------------------

#[test]
fn apply_patch_multi_file_atomic_edit_changes_both_files() {
    let runner = CodelRunner::new()
        .workspace_file("a.txt", "alpha line\n")
        .workspace_file("b.txt", "beta line\n");

    let provider = MockProvider::start()
        .tool_call(
            "apply_patch",
            json!({
                "changes": [
                    {
                        "path": "a.txt",
                        "find": "alpha line",
                        "replace": "ALPHA LINE"
                    },
                    {
                        "path": "b.txt",
                        "find": "beta line",
                        "replace": "BETA LINE"
                    }
                ]
            }),
        )
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Uppercase both files.",
        "--tool-set",
        "edit",
    ]);
    result.assert_success();
    result.assert_tool_called("apply_patch");
    result.assert_turn_completed();

    let a_contents = std::fs::read_to_string(runner.workspace_path().join("a.txt")).unwrap();
    let b_contents = std::fs::read_to_string(runner.workspace_path().join("b.txt")).unwrap();

    assert!(
        a_contents.contains("ALPHA LINE"),
        "a.txt should contain ALPHA LINE, got: {a_contents:?}"
    );
    assert!(
        b_contents.contains("BETA LINE"),
        "b.txt should contain BETA LINE, got: {b_contents:?}"
    );
}

// ---------------------------------------------------------------------------
// apply_patch — atomicity failure
//
// The `apply_patch` implementation in editing.rs applies ALL changes in memory
// first before touching disk (see the two-phase loop: build `contents` BTreeMap,
// then `fs::write` each entry). If any hunk fails to match, it returns
// `Err(HarnessError::ToolFailed)` immediately, before any disk writes happen.
//
// This means: if the batch has one matching hunk and one non-matching hunk and
// the non-matching hunk is evaluated before disk writes, NO file is modified.
// The order of evaluation follows the `changes` array order. We construct the
// batch so the bad hunk comes after the good hunk — both must be resolved in
// memory before anything hits disk, so neither file is written.
//
// The agent loop catches `ToolFailed` and emits `ToolCallFailed` instead of
// `ToolCallCompleted`, then feeds the error back to the model as a tool result.
// ---------------------------------------------------------------------------

#[test]
fn apply_patch_atomicity_failure_leaves_matching_file_unchanged() {
    let runner = CodelRunner::new()
        .workspace_file("good.txt", "good content\n")
        .workspace_file("bad.txt", "bad content\n");

    // The patch has two changes:
    //   1. good.txt — the find text IS present; would succeed.
    //   2. bad.txt  — the find text is NOT present; causes a ToolFailed.
    //
    // Because apply_patch builds all patched contents in memory before writing
    // anything to disk, the failure on bad.txt aborts the batch before good.txt
    // is ever written — leaving good.txt untouched.
    let provider = MockProvider::start()
        .tool_call(
            "apply_patch",
            json!({
                "changes": [
                    {
                        "path": "good.txt",
                        "find": "good content",
                        "replace": "MODIFIED CONTENT"
                    },
                    {
                        "path": "bad.txt",
                        "find": "THIS TEXT DOES NOT EXIST",
                        "replace": "irrelevant"
                    }
                ]
            }),
        )
        .assistant_text("Patch failed, understood.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Apply a patch where one hunk fails.",
        "--tool-set",
        "edit",
    ]);

    // The agent process itself succeeds — ToolCallFailed is a recoverable event;
    // the harness feeds the error message back to the model and continues the loop.
    result.assert_success();

    // A ToolCallFailed event must have been emitted for apply_patch.
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::ToolCallFailed { tool_name, .. } if tool_name == "apply_patch"
        )
    });

    // ToolCallCompleted must NOT have been emitted for apply_patch.
    assert!(
        !result.has_event(|event| {
            matches!(
                event,
                AgentEvent::ToolCallCompleted { tool_name, .. } if tool_name == "apply_patch"
            )
        }),
        "apply_patch should NOT have completed successfully when a hunk fails"
    );

    // Atomicity: because the batch is built entirely in memory before any disk
    // write, good.txt must be unchanged even though its hunk matched.
    let good_contents = std::fs::read_to_string(runner.workspace_path().join("good.txt")).unwrap();
    assert_eq!(
        good_contents, "good content\n",
        "good.txt must be unchanged — the atomic batch was aborted before any disk write"
    );

    // bad.txt is also unchanged (it never matched, so it was never in the write set).
    let bad_contents = std::fs::read_to_string(runner.workspace_path().join("bad.txt")).unwrap();
    assert_eq!(bad_contents, "bad content\n", "bad.txt must be unchanged");
}
