//! End-to-end scenarios for **code execution** (`execute_code`) — SOTA
//! programmatic tool calling — driven through the real `codel00p` binary via
//! `agent run` against a scripted mock provider.
//!
//! `execute_code` lets the model run a short sandboxed Rhai script with arbitrary
//! control flow (loops/conditionals/try-catch) whose tool calls are governed
//! exactly like direct tool calls. These tests prove the CLI wiring and the two
//! properties that matter most through the real binary:
//!
//! - **CLI-reachable.** `--tool-set code` (or `all`) sets `AgentToolSet::Code`,
//!   which makes `build_agent_harness` call `builder.code_execution(true)`,
//!   registering the `execute_code` tool. Its in-script tool surface is a snapshot
//!   of the *other* enabled tool sets (so a script that writes files also needs
//!   e.g. `--tool-set edit`).
//! - **Governed per-call.** With `--permission-mode deny`, the CLI denies the
//!   outer `execute_code` call at the top-level gate, so the script body never
//!   runs and no side effect happens — authority stays with the policy.
//!
//! Fully hermetic: no network, no API keys, isolated `CODEL00P_HOME` + workspace.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

/// **execute_code (CLI-reachable).** A single scripted `execute_code` tool call
/// runs a script that reads a file in a loop and, based on a conditional, creates
/// an output file. Both the read and the governed write happen, and the script's
/// returned value is surfaced — proving control flow drove the governed calls.
#[test]
fn execute_code_runs_a_script_that_reads_then_writes() {
    let runner = CodelRunner::new().workspace_file("input.txt", "PAYLOAD-FROM-SCRIPT\n");

    // One inference emits an `execute_code` call. The script:
    //   - reads input.txt (read-only, governed)
    //   - if its content contains "PAYLOAD", creates out.txt with that content
    //     (workspace write, governed)
    //   - returns the number of bytes copied.
    let provider = MockProvider::start()
        .tool_call(
            "execute_code",
            json!({
                "code": r#"
                    let r = read_file("input.txt");
                    let written = 0;
                    if r.content.contains("PAYLOAD") {
                        create_file("out.txt", r.content);
                        written = r.content.len();
                    }
                    written
                "#
            }),
        )
        .assistant_text("Script complete.");

    let runner = runner.with_provider(&provider);

    // `code` enables execute_code; `edit` puts create_file on the script's
    // in-script tool surface (read_file is always present via read-only defaults).
    let result = runner.run(&[
        "agent",
        "run",
        "Copy input.txt into out.txt with a script.",
        "--tool-set",
        "code",
        "--tool-set",
        "edit",
    ]);
    result.assert_success();

    // Event layer: the code tool ran exactly once and the turn finished.
    result.assert_tool_called("execute_code");
    result.assert_turn_completed();
    assert_eq!(
        provider.hits(),
        2,
        "expected 2 model round-trips (execute_code call + final text)"
    );

    // `execute_code` was advertised to the model alongside its in-script tools.
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = result.assert_context_manifest()
    {
        assert!(
            advertised_tools.iter().any(|t| t == "execute_code"),
            "manifest should advertise execute_code, got {advertised_tools:?}"
        );
        assert!(
            advertised_tools.iter().any(|t| t == "create_file"),
            "manifest should advertise create_file (in-script tool), got {advertised_tools:?}"
        );
    }

    // Side-effect layer proves the governed write ran with the data the script
    // read: out.txt exists only if the conditional branch ran, and its contents
    // equal the read content only if the value flowed through the script.
    let out = runner.workspace_path().join("out.txt");
    assert!(
        out.exists(),
        "out.txt should have been created by the script's create_file call"
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "PAYLOAD-FROM-SCRIPT\n",
        "the script's write should carry the content it read"
    );
}

/// **Per-call governance (CLI-reachable).** With `--permission-mode deny`, the
/// CLI's policy denies every tool — including the outer `execute_code` call,
/// gated at the harness's top-level permission check before the script runs. So
/// the script's write never happens, a `PermissionDenied` event is emitted, and
/// authority stays with the policy.
///
/// The selective per-call case (allow execute_code, deny one in-script tool,
/// prove the denied call alone raises and is skipped) is harness-only and covered
/// by `codel00p-harness/src/code_exec.rs`
/// (`denied_mutation_is_refused_per_call_and_workspace_unchanged`).
#[test]
fn execute_code_denied_under_deny_mode_does_not_run() {
    let runner = CodelRunner::new()
        .workspace_file("input.txt", "PAYLOAD\n")
        .permission_mode("deny");

    let provider = MockProvider::start()
        .tool_call(
            "execute_code",
            json!({
                "code": r#"create_file("denied.txt", "should-not-exist"); "done""#
            }),
        )
        .assistant_text("Acknowledged the denial.");

    let runner = runner.with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Try to write a file with a script.",
        "--tool-set",
        "code",
        "--tool-set",
        "edit",
    ]);
    // The run itself succeeds — a denied tool returns a denial result to the
    // model, it does not abort the process.
    result.assert_success();

    // Authority stayed with the policy: execute_code was requested but denied.
    result.assert_tool_requested("execute_code");
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionDenied { tool_name, .. } if tool_name == "execute_code"
        )
    });
    // The script body never executed, so its write did not happen.
    assert!(
        !runner.workspace_path().join("denied.txt").exists(),
        "the denied script must not have created denied.txt"
    );
}
