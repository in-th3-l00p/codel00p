//! End-to-end scenarios for the **programmatic pipeline** (`run_pipeline`) and
//! **capability synthesis**, driven through the real `codel00p` binary via
//! `agent run` against a scripted mock provider.
//!
//! # What is CLI-reachable vs harness-only
//!
//! Verified against `codel00p-cli/src/agent/{tooling.rs,turn.rs}` and
//! `codel00p-harness/src/agent/builder.rs`:
//!
//! - **`run_pipeline`** — **CLI-reachable.** `--tool-set pipeline` (or `all`)
//!   sets `AgentToolSet::Pipeline`/`All`, which makes `build_agent_harness`
//!   call `builder.programmatic_tooling(true)`, registering the `run_pipeline`
//!   tool. Its sub-step surface is a snapshot of the *other* enabled tool sets
//!   (so a pipeline that writes files also needs e.g. `--tool-set edit`).
//!   Per-step governance is exercised below via the CLI's permission mode.
//!
//! - **`propose_capability`** — **harness-only.** The tool is only registered
//!   when `AgentHarnessBuilder::capability_proposals(sink)` is called, and the
//!   CLI's `build_agent_harness` never calls it (no proposal sink, no
//!   capabilities dir under `CODEL00P_HOME`). So scripting the model to call
//!   `propose_capability` through `agent run` results in an *unknown tool*, not
//!   a written proposal. Asserted negatively below. The positive behavior is
//!   covered in `codel00p-harness/tests/capability_turn.rs`
//!   (`harness_auto_extracts_a_capability_*`) and the unit tests in
//!   `codel00p-harness/src/capability.rs`.
//!
//! - **`verify_capability`** — **harness-only.** It is a library function
//!   (`codel00p_harness::verify_capability`), not a tool and not exposed by any
//!   CLI subcommand. Covered by `verify_capability_*` unit tests in
//!   `codel00p-harness/src/capability.rs`.
//!
//! - **approved `CapabilityTool` loading** — **harness-only.** Capabilities are
//!   registered as tools only via `AgentHarnessBuilder::capabilities(...)`,
//!   which the CLI never calls, and there is no capabilities directory the CLI
//!   loads from. Seeding a capability JSON under `CODEL00P_HOME` therefore does
//!   *not* make it a callable tool. Asserted negatively below. The positive
//!   behavior (a registered capability is advertised and runs its frozen
//!   governed pipeline) is covered by
//!   `codel00p-harness/tests/capability_turn.rs::harness_runs_a_capability_in_one_turn`.
//!
//! Fully hermetic: no network, no API keys, isolated `CODEL00P_HOME` + workspace.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

/// **run_pipeline (CLI-reachable).** A single scripted `run_pipeline` tool call
/// runs two ordered steps where step 1 references step 0's output via
/// `{{steps.0.content}}`. Both steps execute and the templated data flows: the
/// file created by step 1 holds exactly what step 0 read.
#[test]
fn run_pipeline_executes_ordered_steps_and_threads_output() {
    // Seed the source file the pipeline's first step reads.
    let runner = CodelRunner::new().workspace_file("src.txt", "PAYLOAD-FROM-STEP-0\n");

    // One inference emits a `run_pipeline` call with two ordered steps:
    //   step 0 -> read_file(src.txt)               (read-only)
    //   step 1 -> create_file(out.txt,             (workspace write)
    //             content = {{steps.0.content}})    <- references step 0's output
    // A whole-string `{{steps.0.content}}` resolves to the referenced JSON value
    // (read_file returns `{ "path", "content" }`), so out.txt receives step 0's
    // content verbatim.
    let provider = MockProvider::start()
        .tool_call(
            "run_pipeline",
            json!({
                "steps": [
                    { "tool": "read_file", "input": { "path": "src.txt" } },
                    {
                        "tool": "create_file",
                        "input": { "path": "out.txt", "content": "{{steps.0.content}}" }
                    }
                ]
            }),
        )
        .assistant_text("Pipeline complete.");

    let runner = runner.with_provider(&provider);

    // `pipeline` enables run_pipeline; `edit` puts create_file on the pipeline's
    // sub-tool surface (read_file is always present via read-only defaults).
    let result = runner.run(&[
        "agent",
        "run",
        "Copy src.txt into out.txt in one pipeline.",
        "--tool-set",
        "pipeline",
        "--tool-set",
        "edit",
    ]);
    result.assert_success();

    // Event layer: the pipeline tool ran exactly once and the turn finished.
    result.assert_tool_called("run_pipeline");
    result.assert_turn_completed();
    assert_eq!(
        provider.hits(),
        2,
        "expected 2 model round-trips (pipeline call + final text)"
    );

    // `run_pipeline` was advertised to the model alongside its sub-tools.
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = result.assert_context_manifest()
    {
        assert!(
            advertised_tools.iter().any(|t| t == "run_pipeline"),
            "manifest should advertise run_pipeline, got {advertised_tools:?}"
        );
        assert!(
            advertised_tools.iter().any(|t| t == "create_file"),
            "manifest should advertise create_file (pipeline sub-tool), got {advertised_tools:?}"
        );
    }

    // Side-effect layer proves BOTH steps ran AND the templated data flowed:
    // out.txt exists only if step 1 ran, and its contents equal step 0's read
    // only if `{{steps.0.content}}` resolved through the pipeline context.
    let out = runner.workspace_path().join("out.txt");
    assert!(
        out.exists(),
        "out.txt should have been created by the pipeline's second step"
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "PAYLOAD-FROM-STEP-0\n",
        "step 1's content should be step 0's output, threaded via {{{{steps.0.content}}}}"
    );
}

/// **Per-step governance (CLI-reachable).** With `--permission-mode deny`, the
/// CLI's `CliPermissionPolicy` denies every tool — including the outer
/// `run_pipeline` call, which is gated at the harness's top-level permission
/// check before any step runs. So the pipeline's write never happens and
/// authority stays with the policy: a `PermissionDenied` event is emitted and
/// no file is created.
///
/// Note: the CLI's `deny` mode is all-or-nothing, so it denies `run_pipeline`
/// at the *outer* gate rather than selectively denying one step inside an
/// otherwise-allowed pipeline. The selective per-step case (allow the pipeline,
/// deny one inner tool, prove the denied step alone is skipped) is harness-only
/// and covered by
/// `codel00p-harness/tests/pipeline_turn.rs::pipeline_respects_the_harness_permission_policy`.
#[test]
fn pipeline_step_denied_under_deny_mode_does_not_run() {
    let runner = CodelRunner::new()
        .workspace_file("src.txt", "PAYLOAD\n")
        .permission_mode("deny");

    let provider = MockProvider::start()
        .tool_call(
            "run_pipeline",
            json!({
                "stop_on_error": false,
                "steps": [
                    {
                        "tool": "create_file",
                        "input": { "path": "denied.txt", "content": "should-not-exist" }
                    }
                ]
            }),
        )
        .assistant_text("Acknowledged the denial.");

    let runner = runner.with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Try to write a file through a pipeline.",
        "--tool-set",
        "pipeline",
        "--tool-set",
        "edit",
    ]);
    // The run itself succeeds — a denied tool returns a denial result to the
    // model, it does not abort the process.
    result.assert_success();

    // Authority stayed with the policy: run_pipeline was requested but denied.
    result.assert_tool_requested("run_pipeline");
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionDenied { tool_name, .. } if tool_name == "run_pipeline"
        )
    });
    // The pipeline body never executed, so its write did not happen.
    assert!(
        !runner.workspace_path().join("denied.txt").exists(),
        "the denied pipeline must not have created denied.txt"
    );
}

/// **propose_capability (harness-only — asserted negatively).** The CLI never
/// wires a `CapabilityProposalSink`, so `propose_capability` is not a tool the
/// `agent run` path advertises or accepts. Scripting the model to call it
/// surfaces an unknown tool; nothing is written to any proposal sink. We assert
/// the negative explicitly so this stays true: no proposal file lands anywhere
/// under `CODEL00P_HOME`, and `propose_capability` is not advertised.
///
/// The positive flow (a proposal is validated and queued, not auto-registered)
/// lives in `codel00p-harness/src/capability.rs`
/// (`propose_capability_validates_and_queues`, `file_sink_roundtrips_through_load`)
/// and `codel00p-harness/tests/capability_turn.rs`.
#[test]
fn propose_capability_is_not_reachable_through_agent_run() {
    let runner = CodelRunner::new().workspace_file("a.rs", "// a\n");

    // Script a `propose_capability` call followed by final text. Whatever the
    // unknown-tool result is, the run must still finish; the point is that no
    // proposal is persisted.
    let provider = MockProvider::start()
        .tool_call(
            "propose_capability",
            json!({
                "name": "scaffold_module",
                "description": "Create a source file and its test",
                "parameters": { "type": "object", "properties": {} },
                "steps": [
                    { "tool": "create_file", "input": { "path": "src/{{params.x}}.rs", "content": "x" } },
                    { "tool": "create_file", "input": { "path": "tests/{{params.x}}.rs", "content": "y" } }
                ]
            }),
        )
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Freeze this into a capability.",
        "--tool-set",
        "all",
    ]);
    result.assert_success();

    // `propose_capability` is NOT advertised by the CLI agent.
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = result.assert_context_manifest()
    {
        assert!(
            !advertised_tools.iter().any(|t| t == "propose_capability"),
            "propose_capability must not be advertised by the CLI (harness-only), got {advertised_tools:?}"
        );
    }

    // No capability proposal was written anywhere under CODEL00P_HOME (the CLI
    // has no proposal sink). Scan the whole home tree for a `scaffold_module`
    // capability JSON to be robust to where a sink *would* land.
    assert!(
        !home_contains_capability_json(runner.home_path(), "scaffold_module"),
        "no capability proposal should be persisted: propose_capability is harness-only"
    );
}

/// **approved CapabilityTool loading (harness-only — asserted negatively).**
/// Seed an approved capability JSON under `CODEL00P_HOME` and confirm the CLI
/// does NOT load it as a callable tool: it is not advertised and the model
/// cannot invoke it. Capabilities become tools only via
/// `AgentHarnessBuilder::capabilities(...)`, which the CLI never calls.
///
/// The positive flow (a registered capability is advertised and, when called,
/// runs its frozen governed pipeline) is covered by
/// `codel00p-harness/tests/capability_turn.rs::harness_runs_a_capability_in_one_turn`.
#[test]
fn approved_capability_json_is_not_loaded_by_the_cli() {
    // Seed an approved-looking capability in a plausible capabilities dir under
    // CODEL00P_HOME. The CLI has no loader, so this should be inert.
    let capability = json!({
        "name": "scaffold_module",
        "description": "Create a source file and its test file for a module",
        "parameters": {
            "type": "object",
            "required": ["name"],
            "properties": { "name": { "type": "string" } }
        },
        "steps": [
            { "tool": "create_file", "input": { "path": "src/{{params.name}}.rs", "content": "// {{params.name}}\n" } },
            { "tool": "create_file", "input": { "path": "tests/{{params.name}}_test.rs", "content": "// {{params.name}}\n" } }
        ]
    });

    let runner = CodelRunner::new();
    let caps_dir = runner.home_path().join("capabilities");
    std::fs::create_dir_all(&caps_dir).expect("create capabilities dir");
    std::fs::write(
        caps_dir.join("scaffold_module.json"),
        serde_json::to_string_pretty(&capability).unwrap(),
    )
    .expect("seed capability json");

    // Try to call the (seeded) capability by name. The CLI never loaded it, so
    // it is not a real tool — but the run should still complete.
    let provider = MockProvider::start()
        .tool_call("scaffold_module", json!({ "name": "widget" }))
        .assistant_text("Done.");

    let runner = runner.with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Use the scaffold_module capability.",
        "--tool-set",
        "all",
    ]);
    result.assert_success();

    // The seeded capability is NOT advertised as a tool.
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = result.assert_context_manifest()
    {
        assert!(
            !advertised_tools.iter().any(|t| t == "scaffold_module"),
            "seeded capability must not be advertised: CLI does not load capabilities, got {advertised_tools:?}"
        );
    }

    // It did not run as a frozen pipeline: neither scaffolded file exists.
    assert!(
        !runner.workspace_path().join("src/widget.rs").exists(),
        "the capability must not have executed (CLI does not load capabilities)"
    );
    assert!(
        !runner
            .workspace_path()
            .join("tests/widget_test.rs")
            .exists(),
        "the capability must not have executed (CLI does not load capabilities)"
    );
}

/// Recursively scans `home` for a `<name>.json` file that parses as a capability
/// proposal (has the expected `name`). Used to assert no proposal was persisted.
fn home_contains_capability_json(home: &std::path::Path, name: &str) -> bool {
    fn walk(dir: &std::path::Path, name: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if walk(&path, name) {
                    return true;
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                && value.get("name").and_then(|v| v.as_str()) == Some(name)
                && value.get("steps").is_some()
            {
                return true;
            }
        }
        false
    }
    walk(home, name)
}
