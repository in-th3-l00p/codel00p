//! End-to-end turn-control scenarios driven through the real `codel00p` binary.
//!
//! This group documents which turn-control knobs are reachable from the **CLI**
//! `agent run` surface versus which are *harness-only* (settable on
//! `AgentHarness::builder()` but with no `agent run` flag). Every scenario drives
//! the real binary against a scripted [`MockProvider`] and asserts on observable
//! outcomes — the number of model round-trips (`MockProvider::hits()`), the raw
//! request bodies the model was shown (`MockProvider::received_requests()`), the
//! emitted `--json-events` protocol events, and process exit status.
//!
//! # CLI-reachable turn-control knobs (asserted here)
//!
//! - **Multi-iteration tool loop** — inherent; the agent re-calls the model after
//!   each tool result until a final text turn. Asserted via round-trip count and
//!   the `iterations` field on `TurnCompleted`.
//! - **`--max-iterations <N>`** — caps inference iterations. When every iteration
//!   produces tool calls and the cap is hit before a final text turn, the turn
//!   returns `HarnessError::IterationLimit` and the binary exits non-zero with
//!   "turn reached iteration limit: N" on stderr. Confirmed from
//!   `codel00p-harness/src/agent/turn.rs` (the `while budget.consume()` loop ends
//!   in `Err(IterationLimit { limit })`) and `iteration_budget.rs`.
//! - **Tool-result truncation** — the recorded-for-model tool result is capped at
//!   the harness default of 16 KiB (`codel00p-harness/src/agent/builder.rs`:
//!   `max_tool_result_bytes` default `16 * 1024`) with a `codel00p truncated`
//!   head+tail marker (`codel00p-harness/src/truncation.rs`). The cap itself is
//!   **not** a CLI flag (`max_tool_result_bytes` is builder-only), so this test
//!   asserts the DEFAULT cap the CLI produces.
//! - **`--stream-events`** — emits protocol events live during the turn (in
//!   addition to the buffered `--json-events` dump the runner always requests),
//!   so the streamed run shows strictly more event lines than buffered-only.
//! - **`--json-events` ordering** — the emitted protocol-event stream for a
//!   fixed script is deterministic in shape (opens before the first tool, one
//!   closing `TurnCompleted`, each tool request before its completion).
//!
//! # CLI-reachable but NOT serveable by this hermetic mock
//!
//! - **`--stream`** (token streaming) IS a real `agent run` flag, but it drives
//!   the provider's SSE path (`chat_completions::complete_streaming`). The
//!   [`MockProvider`] serves only a single buffered chat-completions JSON body and
//!   does not speak SSE, so `--stream` errors with "stream produced no events"
//!   against this mock and cannot be asserted deterministically. Token streaming
//!   is covered at the transport layer in `codel00p-providers`. We assert
//!   `--json-events` ordering stability instead (the recommended fallback).
//!
//! # Harness-only knobs (NOT CLI-reachable — documented, not faked)
//!
//! - **`tool_choice`** (auto / required / specific / none) and **`response_format`**
//!   / JSON mode are `AgentHarness::builder()` methods
//!   (`codel00p-harness/src/agent/builder.rs`: `tool_choice`, `response_format`)
//!   that the turn loop forwards onto the inference request
//!   (`turn.rs`: `with_tool_choice` / `with_response_format`). There is **no**
//!   `agent run` flag for either (verified: no match in `codel00p-cli/src`). They
//!   are covered by harness-level tests in
//!   `codel00p-harness/tests/provider_adapter.rs`. The note test below asserts the
//!   *consequence* of this: the request the model receives on the default CLI path
//!   carries neither a `tool_choice` nor a `response_format` field.
//!
//! Fully hermetic: no network, no API keys, isolated `CODEL00P_HOME` + workspace.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::{Value, json};

/// Multi-iteration tool loop: three tool-call turns then a final text turn drive
/// four model round-trips, and `TurnCompleted` reports the loop ran four
/// iterations (the final text arrived on iteration 4).
#[test]
fn multi_iteration_loop_runs_expected_round_trips() {
    let provider = MockProvider::start()
        .tool_call("create_file", json!({ "path": "a.txt", "content": "a\n" }))
        .tool_call("create_file", json!({ "path": "b.txt", "content": "b\n" }))
        .tool_call("create_file", json!({ "path": "c.txt", "content": "c\n" }))
        .assistant_text("created three files");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Create three files.", "--tool-set", "all"]);
    result.assert_success();

    // One round-trip per scripted turn: 3 tool turns + 1 final text turn.
    assert_eq!(
        provider.hits(),
        4,
        "expected exactly four model round-trips, got {}",
        provider.hits()
    );

    // Each scripted tool ran and the loop emitted a completion.
    result.assert_tool_called("create_file");
    result.assert_turn_completed();

    // The final text arrived on the 4th iteration; `TurnCompleted.iterations`
    // reports that (1-based, per `IterationBudget::used()`).
    let iterations = result
        .events()
        .iter()
        .find_map(|event| match event {
            AgentEvent::TurnCompleted { iterations, .. } => Some(*iterations),
            _ => None,
        })
        .expect("a TurnCompleted event");
    assert_eq!(
        iterations, 4,
        "final text turn arrived on iteration 4, got {iterations}"
    );

    // Side effect: all three files exist.
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert!(
            runner.workspace_path().join(name).exists(),
            "{name} should have been created"
        );
    }
}

/// `--max-iterations` cap: script more tool turns than allowed and never a final
/// text turn. The loop runs exactly `--max-iterations` inference iterations, then
/// stops with an iteration-limit error and a non-zero exit. Stop semantics
/// confirmed from `turn.rs` (`while budget.consume()` → `Err(IterationLimit)`).
#[test]
fn max_iterations_cap_stops_the_loop_and_fails() {
    // Three tool turns are scripted, but the cap is 2: only turns 0 and 1 are ever
    // served, then the loop exhausts its budget before reaching any final text.
    let provider = MockProvider::start()
        .tool_call("create_file", json!({ "path": "a.txt", "content": "a\n" }))
        .tool_call("create_file", json!({ "path": "b.txt", "content": "b\n" }))
        .tool_call("create_file", json!({ "path": "c.txt", "content": "c\n" }))
        .assistant_text("never reached");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Create files forever.",
        "--tool-set",
        "all",
        "--max-iterations",
        "2",
    ]);

    // The loop stopped at the cap: exactly two model round-trips, no more.
    assert_eq!(
        provider.hits(),
        2,
        "loop should stop at the 2-iteration cap, got {} round-trips",
        provider.hits()
    );

    // Hitting the cap without a final text turn is a turn failure: the binary
    // exits non-zero and surfaces the iteration-limit error on stderr.
    assert!(
        !result.success(),
        "command should fail when the iteration cap is hit before a final turn\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        result.stdout(),
        result.stderr()
    );
    assert!(
        result.stderr().contains("turn reached iteration limit: 2"),
        "stderr should report the iteration limit, got:\n{}",
        result.stderr()
    );
}

/// Tool-result truncation: a `run_command` whose output exceeds the harness's
/// default 16 KiB tool-result cap is truncated before it is recorded for the
/// model. We assert on the *next* request the mock received — the one carrying
/// the recorded tool result — which must contain the truncation marker rather
/// than the full output. The cap (16 KiB) and marker text are harness defaults
/// (`builder.rs` + `truncation.rs`); the cap is not CLI-settable.
#[test]
fn tool_result_truncation_caps_model_visible_output() {
    // Seed a file well over the 16 KiB cap so `cat` produces oversized output.
    // The command tool's own output cap defaults to 16 KiB too, so we raise it via
    // `max_output_bytes` to let the large output through to the harness layer,
    // which is the layer under test.
    let big_len = 40_000usize;
    let big_content = "X".repeat(big_len);

    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({
                "program": "cat",
                "args": ["big.txt"],
                "max_output_bytes": 131072,
            }),
        )
        .assistant_text("read the big file");

    let runner = CodelRunner::new()
        .workspace_file("big.txt", &big_content)
        .with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Read the big file.",
        "--tool-set",
        "command",
    ]);
    result.assert_success();
    result.assert_tool_called("run_command");

    // The request that carried the recorded tool result back to the model is the
    // second one (index 1): it includes the `role:"tool"` message.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2, "expected two model round-trips");
    let follow_up = &requests[1];

    // The model-visible result was truncated: the head+tail marker is present and
    // the full 40 KB payload is NOT shown verbatim.
    assert!(
        follow_up.contains("codel00p truncated"),
        "follow-up request should carry the truncation marker, body:\n{follow_up}"
    );
    assert!(
        !follow_up.contains(&"X".repeat(big_len)),
        "the full untruncated payload must not be shown to the model"
    );
}

/// `--stream-events` emits protocol events live during the turn. Because the
/// runner always also passes `--json-events` (which dumps the buffered events at
/// the end), a `--stream-events` run shows strictly more parsed event lines than
/// the same script without it.
#[test]
fn stream_events_emits_more_event_lines_than_buffered_only() {
    let buffered_provider = MockProvider::start()
        .tool_call("create_file", json!({ "path": "x.txt", "content": "x\n" }))
        .assistant_text("done");
    let buffered_runner = CodelRunner::new().with_provider(&buffered_provider);
    let buffered = buffered_runner.run(&["agent", "run", "Go.", "--tool-set", "all"]);
    buffered.assert_success();

    let streamed_provider = MockProvider::start()
        .tool_call("create_file", json!({ "path": "x.txt", "content": "x\n" }))
        .assistant_text("done");
    let streamed_runner = CodelRunner::new().with_provider(&streamed_provider);
    let streamed = streamed_runner.run(&[
        "agent",
        "run",
        "Go.",
        "--tool-set",
        "all",
        "--stream-events",
    ]);
    streamed.assert_success();

    // Same model round-trips either way: streaming changes output shape, not the
    // protocol conversation.
    assert_eq!(buffered_provider.hits(), 2);
    assert_eq!(streamed_provider.hits(), 2);

    // The streamed run emits events twice (live during the turn + the buffered
    // end-of-turn dump), so it has strictly more parsed event lines.
    assert!(
        streamed.events().len() > buffered.events().len(),
        "streamed events ({}) should exceed buffered events ({})",
        streamed.events().len(),
        buffered.events().len()
    );
}

/// `--json-events` ordering is stable and well-formed: the emitted event stream
/// for a fixed script is deterministic in shape — it opens before the first tool
/// runs and a single `TurnCompleted` closes it, after every tool completion.
///
/// Note on `--stream` (token streaming): the flag IS CLI-reachable
/// (`agent run --stream`, parsed in `codel00p-cli/src/agent/options.rs`), but it
/// drives the provider's SSE path (`chat_completions::complete_streaming`, which
/// reads `data:` chunks). The hermetic [`MockProvider`] serves only a single
/// buffered chat-completions JSON body and does **not** speak SSE, so `--stream`
/// errors with "stream produced no events" against this mock and cannot be
/// asserted deterministically here. Token streaming is covered at the transport
/// layer in `codel00p-providers`. We therefore assert `--json-events` ordering
/// stability (the recommended fallback) instead.
#[test]
fn json_events_ordering_is_stable_and_well_formed() {
    let provider = MockProvider::start()
        .tool_call("create_file", json!({ "path": "x.txt", "content": "x\n" }))
        .assistant_text("done");
    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Go.", "--tool-set", "all"]);
    result.assert_success();

    let events = result.events();
    assert!(!events.is_empty(), "expected emitted protocol events");

    // The stream opens before any tool runs.
    let first_tool_request = events
        .iter()
        .position(|e| matches!(e, AgentEvent::ToolCallRequested { .. }))
        .expect("a ToolCallRequested event");
    let session_started = events.iter().position(|e| {
        matches!(
            e,
            AgentEvent::SessionStarted { .. } | AgentEvent::TurnStarted { .. }
        )
    });
    if let Some(start) = session_started {
        assert!(
            start < first_tool_request,
            "session/turn start should precede the first tool request"
        );
    }

    // Exactly one TurnCompleted, and it is the final event.
    let completed_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::TurnCompleted { .. }))
        .count();
    assert_eq!(completed_count, 1, "exactly one TurnCompleted");
    assert!(
        matches!(events.last(), Some(AgentEvent::TurnCompleted { .. })),
        "TurnCompleted should be the last event, got {:?}",
        events.last()
    );

    // Every requested tool is completed before the turn closes, and the
    // request precedes its completion.
    let req_idx = events
        .iter()
        .position(|e| matches!(e, AgentEvent::ToolCallRequested { tool_name, .. } if tool_name == "create_file"))
        .expect("create_file requested");
    let done_idx = events
        .iter()
        .position(|e| matches!(e, AgentEvent::ToolCallCompleted { tool_name, .. } if tool_name == "create_file"))
        .expect("create_file completed");
    assert!(req_idx < done_idx, "request precedes completion");
}

/// Documented harness-only note: `tool_choice` and `response_format` are NOT
/// CLI-reachable. The default `agent run` path sets neither on the inference
/// request, so the request the mock receives must carry no `tool_choice` and no
/// `response_format` field. This documents the boundary: those knobs are
/// builder-only (see `codel00p-harness/src/agent/builder.rs`) and are exercised by
/// `codel00p-harness/tests/provider_adapter.rs`, not by any `agent run` flag.
#[test]
fn tool_choice_and_response_format_are_not_set_on_default_cli_path() {
    let provider = MockProvider::start().assistant_text("hello");
    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 1, "expected a single model round-trip");
    let body: Value =
        serde_json::from_str(&requests[0]).expect("request body should be valid JSON");
    let obj = body.as_object().expect("request body is a JSON object");

    assert!(
        !obj.contains_key("tool_choice"),
        "tool_choice is harness-only and must not appear on the default CLI path: {body}"
    );
    assert!(
        !obj.contains_key("response_format"),
        "response_format is harness-only and must not appear on the default CLI path: {body}"
    );
}
