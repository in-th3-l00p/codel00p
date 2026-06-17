//! End-to-end tests for the command-execution tool group.
//!
//! Each scenario drives the real `codel00p` binary against a scripted mock
//! provider and asserts over emitted protocol events plus the tool-result
//! content that the harness feeds back to the model (extracted from the mock's
//! captured request bodies).
//!
//! Tools under test: `run_command`, `process_output`, `process_list`,
//! `process_kill`.
//!
//! # Background-output timing
//!
//! The `process_id` returned by `run_command background:true` is deterministic
//! within a single session: `BackgroundProcesses::spawn` increments an atomic
//! counter starting at 1, so the first background process is always `proc-1`.
//! This lets us pre-script the mock's subsequent turns with that id. We do NOT
//! assert on exact freshly-flushed stdout from a background process (that
//! would be racy); we assert on lifecycle: the launch was acknowledged, the
//! process appears in `process_list`, and `process_kill` terminates it.

use codel00p_e2e::{CodelRunner, MockProvider};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Helper: extract the tool-result content from the Nth mock request body.
//
// The mock captures each `POST /chat/completions` request body in order.
// Request index N carries the tool results from the previous N rounds of tool
// calls as `{"role": "tool", "content": "<json>"}` messages at the tail of the
// `messages` array.  We find the last tool-role message in request `req_index`
// and parse its `content` field as JSON.
// ---------------------------------------------------------------------------
fn last_tool_result(provider: &MockProvider, req_index: usize) -> Value {
    let requests = provider.received_requests();
    let body: Value =
        serde_json::from_str(&requests[req_index]).expect("request body should be valid JSON");
    let messages = body["messages"]
        .as_array()
        .expect("messages array in request body");
    let last_tool_msg = messages
        .iter()
        .rev()
        .find(|m| m["role"] == "tool")
        .expect("at least one tool-role message");
    let content_str = last_tool_msg["content"]
        .as_str()
        .expect("tool content is a JSON string");
    serde_json::from_str(content_str).expect("tool content parses as JSON")
}

// ---------------------------------------------------------------------------
// 1. Foreground run_command — success path
// ---------------------------------------------------------------------------
#[test]
fn foreground_run_command_success_echo() {
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({ "program": "sh", "args": ["-c", "echo hello-e2e"] }),
        )
        .assistant_text("done");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Echo something.",
        "--tool-set",
        "command",
    ]);

    result.assert_success();
    result.assert_tool_called("run_command");
    result.assert_turn_completed();

    // The model's second request carries the run_command result.
    let tool_result = last_tool_result(&provider, 1);
    assert_eq!(
        tool_result["success"], true,
        "echo should succeed; tool_result: {tool_result}"
    );
    assert_eq!(
        tool_result["exit_code"], 0,
        "exit code should be 0; tool_result: {tool_result}"
    );
    assert!(
        tool_result["stdout"]
            .as_str()
            .unwrap_or("")
            .contains("hello-e2e"),
        "stdout should contain 'hello-e2e'; tool_result: {tool_result}"
    );
    assert_eq!(
        tool_result["timed_out"], false,
        "should not have timed out; tool_result: {tool_result}"
    );
}

// ---------------------------------------------------------------------------
// 2. Foreground run_command — non-zero exit does not crash the harness
// ---------------------------------------------------------------------------
#[test]
fn foreground_run_command_nonzero_exit_is_reported_not_crash() {
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({ "program": "sh", "args": ["-c", "exit 42"] }),
        )
        .assistant_text("command failed as expected");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Run a failing command.",
        "--tool-set",
        "command",
    ]);

    // The harness must not crash — the agent run should still exit cleanly.
    result.assert_success();
    result.assert_tool_called("run_command");
    result.assert_turn_completed();

    let tool_result = last_tool_result(&provider, 1);
    assert_eq!(
        tool_result["success"], false,
        "non-zero exit must be reported as not successful; tool_result: {tool_result}"
    );
    assert_eq!(
        tool_result["exit_code"], 42,
        "exit code 42 should be captured; tool_result: {tool_result}"
    );
    assert_eq!(
        tool_result["timed_out"], false,
        "should not have timed out; tool_result: {tool_result}"
    );
}

// ---------------------------------------------------------------------------
// 3. Foreground run_command — timeout on a slow command
// ---------------------------------------------------------------------------
#[test]
fn foreground_run_command_timeout() {
    // timeout_ms = 100 on a `sleep 30` — the harness must kill and report
    // timed_out rather than blocking for 30 s.
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({
                "program": "sleep",
                "args": ["30"],
                "timeout_ms": 100
            }),
        )
        .assistant_text("timed out as expected");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Run a slow command with a short timeout.",
        "--tool-set",
        "command",
    ]);

    result.assert_success();
    result.assert_tool_called("run_command");
    result.assert_turn_completed();

    let tool_result = last_tool_result(&provider, 1);
    assert_eq!(
        tool_result["timed_out"], true,
        "command should have timed out; tool_result: {tool_result}"
    );
    assert_eq!(
        tool_result["success"], false,
        "timed-out command should not be success; tool_result: {tool_result}"
    );
    // When timed out the exit_code is null (None).
    assert!(
        tool_result["exit_code"].is_null(),
        "exit_code should be null for timed-out command; tool_result: {tool_result}"
    );
}

// ---------------------------------------------------------------------------
// 4. Foreground run_command — output cap triggers truncation flag
// ---------------------------------------------------------------------------
#[test]
fn foreground_run_command_output_cap_truncates() {
    // Generate 100 bytes with yes | head then cap at 10 bytes → truncated.
    // `printf 'x%.0s' {1..100}` is portable; use python for reliability.
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({
                "program": "sh",
                "args": ["-c", "printf '%0.s1234567890' 1 2 3 4 5 6 7 8 9 10 11 12"],
                "max_output_bytes": 10
            }),
        )
        .assistant_text("output was truncated");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Run a command that produces lots of output.",
        "--tool-set",
        "command",
    ]);

    result.assert_success();
    result.assert_tool_called("run_command");
    result.assert_turn_completed();

    let tool_result = last_tool_result(&provider, 1);
    assert_eq!(
        tool_result["stdout_truncated"], true,
        "stdout should be marked truncated; tool_result: {tool_result}"
    );
    // The captured portion must not exceed the cap.
    let stdout_len = tool_result["stdout"].as_str().unwrap_or("").len();
    assert!(
        stdout_len <= 10,
        "captured stdout ({stdout_len} bytes) should be <= 10; tool_result: {tool_result}"
    );
}

// ---------------------------------------------------------------------------
// 5. Background flow — launch, list, kill lifecycle
//
// The process_id is deterministic: the first background process in a fresh
// session is always `proc-1` (BackgroundProcesses counter starts at 1).
//
// We do NOT assert on freshly-flushed stdout (race-prone). Instead we verify:
//   turn 0: run_command background:true → acknowledged + process_id = "proc-1"
//   turn 1: process_list → shows exactly one running process with id "proc-1"
//   turn 2: process_kill "proc-1" → status is "killed"
//   turn 3: assistant stops
// ---------------------------------------------------------------------------
#[test]
fn background_launch_list_kill_lifecycle() {
    // A process that prints one line then sleeps long enough not to exit before
    // we kill it.
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({
                "program": "sh",
                "args": ["-c", "echo started; sleep 30"],
                "background": true
            }),
        )
        .tool_call("process_list", json!({}))
        .tool_call("process_kill", json!({ "process_id": "proc-1" }))
        .assistant_text("process killed");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Start a background process, list it, then kill it.",
        "--tool-set",
        "command",
    ]);

    result.assert_success();
    result.assert_tool_called("run_command");
    result.assert_tool_called("process_list");
    result.assert_tool_called("process_kill");
    result.assert_turn_completed();

    // Turn 0 result: run_command with background:true → acknowledged.
    let launch_result = last_tool_result(&provider, 1);
    assert_eq!(
        launch_result["background"], true,
        "background flag should be true; result: {launch_result}"
    );
    assert_eq!(
        launch_result["process_id"], "proc-1",
        "first background process should get id proc-1; result: {launch_result}"
    );
    assert_eq!(
        launch_result["status"], "running",
        "initial status should be running; result: {launch_result}"
    );

    // Turn 1 result: process_list → one running process with our id.
    let list_result = last_tool_result(&provider, 2);
    let processes = list_result["processes"]
        .as_array()
        .expect("processes should be an array");
    assert_eq!(
        processes.len(),
        1,
        "exactly one background process should be listed; result: {list_result}"
    );
    assert_eq!(
        processes[0]["process_id"], "proc-1",
        "listed process should have id proc-1; result: {list_result}"
    );
    assert_eq!(
        processes[0]["status"]["state"], "running",
        "process should still be running at list time; result: {list_result}"
    );

    // Turn 2 result: process_kill → status is killed.
    let kill_result = last_tool_result(&provider, 3);
    assert_eq!(
        kill_result["process_id"], "proc-1",
        "killed process id should be proc-1; result: {kill_result}"
    );
    assert_eq!(
        kill_result["status"]["state"], "killed",
        "process should be reported as killed; result: {kill_result}"
    );
    assert_eq!(
        kill_result["status"]["killed"], true,
        "killed flag should be true; result: {kill_result}"
    );
}

// ---------------------------------------------------------------------------
// 6. process_output — called after background launch
//
// Same timing note as above: we assert on the process_id and status fields,
// not on exact freshly-flushed stdout. We give the process a moment to write
// output by sleeping briefly before calling process_output; rather than using
// thread::sleep in the test, we let the harness drive the tool naturally (the
// background reader thread drains pipes continuously).
// ---------------------------------------------------------------------------
#[test]
fn background_process_output_poll() {
    // Use a process that exits quickly, so by the time the mock's turn 1 runs
    // process_output the process has already exited and its output is drained.
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({
                "program": "sh",
                // Print output then exit immediately; no lingering sleep.
                "args": ["-c", "echo bg-output-poll"],
                "background": true
            }),
        )
        // poll process_output; the process will have exited by the time this
        // turn executes (no sleep needed — the tool call round-trip takes >1ms
        // and the echo exits near-instantly).
        .tool_call("process_output", json!({ "process_id": "proc-1" }))
        .assistant_text("output polled");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Start a short background process then poll its output.",
        "--tool-set",
        "command",
    ]);

    result.assert_success();
    result.assert_tool_called("run_command");
    result.assert_tool_called("process_output");
    result.assert_turn_completed();

    // process_output result: check process_id and that the call succeeded.
    let output_result = last_tool_result(&provider, 2);
    assert_eq!(
        output_result["process_id"], "proc-1",
        "process_output should report proc-1; result: {output_result}"
    );
    // The status should be "exited" (process ran echo then exited immediately),
    // OR still "running" if the OS hasn't scheduled the exit yet — we accept
    // both to avoid flakiness, but we must have a valid state field.
    let state = output_result["status"]["state"]
        .as_str()
        .expect("status.state should be a string");
    assert!(
        state == "running" || state == "exited",
        "status.state should be running or exited; got {state:?}; result: {output_result}"
    );
}

// ---------------------------------------------------------------------------
// 7. Context manifest advertises command tools
// ---------------------------------------------------------------------------
#[test]
fn context_manifest_advertises_command_tools() {
    let provider = MockProvider::start().assistant_text("nothing to do");

    let result = CodelRunner::new().with_provider(&provider).run(&[
        "agent",
        "run",
        "Do nothing.",
        "--tool-set",
        "command",
    ]);

    result.assert_success();
    result.assert_turn_completed();

    let manifest = result.assert_context_manifest();
    if let codel00p_e2e::AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        for name in &[
            "run_command",
            "process_list",
            "process_kill",
            "process_output",
        ] {
            assert!(
                advertised_tools.iter().any(|t| t == name),
                "manifest should advertise {name}; got {advertised_tools:?}"
            );
        }
    } else {
        panic!("expected ContextManifest event");
    }
}
