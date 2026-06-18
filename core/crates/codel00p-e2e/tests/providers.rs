//! End-to-end provider-behavior scenarios driven through the real `codel00p`
//! binary, fully hermetic (a scripted mock HTTP provider; no live keys, no
//! network).
//!
//! This group documents which provider behaviors are reachable from the **real
//! binary** (the `agent run` inference path and the `config providers` catalog
//! surface) versus which live purely in the `codel00p-providers` library and are
//! therefore **not** triggerable through the product's CLI/config/env. Every
//! reachable behavior is asserted on an observable outcome (process exit, model
//! round-trip count via `MockProvider::hits()`, emitted `--json-events`, and
//! command stdout). Everything that is *not* hermetically reachable is documented
//! here (with the library-level coverage it already has) rather than faked.
//!
//! # Reachable through the real binary (asserted here)
//!
//! - **Retry / backoff on transient errors** — the `agent run` inference client is
//!   built with the default [`codel00p_providers::RetryPolicy`] (two retries; see
//!   `codel00p-providers/src/client/retry.rs`). A `429`/`503`-class status is
//!   classified *retryable* (`codel00p-providers/src/error_classifier.rs`), so the
//!   binary retries the *same* `POST /chat/completions` route and ultimately
//!   succeeds. The mock advertises `Retry-After: 0`, which the policy honors as the
//!   delay (`RetryPolicy::delay`), so the retry is immediate and the test is fast.
//!   Asserted via the model round-trip count (the failed attempt + the retry that
//!   succeeds) and a successful exit.
//!
//! - **Model catalog / provider listing** — `codel00p config providers list` and
//!   `codel00p config providers show <id>` read the static built-in provider
//!   registry (`default_registry()`); they are hermetic (no network, no key) and
//!   print provider ids, API mode, and streaming capability. Asserted on stdout.
//!   These are plain (non-`agent`) subcommands, so the harness does **not**
//!   auto-inject provider/model/base-url flags — they run via `.run([...])` as-is.
//!
//! # NOT reachable through the real binary (documented, not faked)
//!
//! - **Fallback routing between providers** — fallback is a per-request route chain
//!   (`InferenceRequest::fallback_route_with_base_url`,
//!   `codel00p-providers/src/request.rs`) consumed by `InferenceClient::complete`.
//!   There is **no** CLI flag, config-TOML key, or env var to populate it: a repo
//!   search for `fallback_route` / `fallback` finds zero call sites in
//!   `codel00p-cli/src` or `codel00p-harness/src`, and the harness's provider
//!   adapter builds the request without ever appending a fallback route. So the
//!   real binary never sets a fallback route and the behavior cannot be triggered
//!   end-to-end. It is covered at the library layer in
//!   `codel00p-providers/tests/fallback_routing.rs`
//!   (`falls_back_when_primary_route_is_rate_limited`,
//!   `does_not_fallback_for_non_fallbackable_errors`) and
//!   `codel00p-providers/tests/retry_backoff.rs`
//!   (`retries_the_same_route_then_falls_back`). The `#[ignore]`d note test below
//!   records this boundary explicitly.
//!
//! - **Usage / cost reporting** — token usage and an estimated cost are parsed and
//!   attached to the provider response (`Usage` / `UsageCostEstimate` on
//!   `InferenceResponse`, `codel00p-providers/src/response.rs`), and cost is only
//!   estimated when pricing is supplied (request pricing, client `model_pricing`,
//!   or a pricing catalog). But the `AgentEvent` protocol carries **no** usage or
//!   cost field (`codel00p-protocol/src/events.rs`: `TurnCompleted` reports only
//!   `iterations`), and the CLI does not print usage in `--json-events` or stdout.
//!   So usage/cost is not observable through the binary and is covered at the
//!   library layer in `codel00p-providers/tests/usage_cost.rs`. The note test below
//!   asserts the *consequence*: no emitted event exposes usage/cost.
//!
//! Fully hermetic: no network, no API keys, isolated `CODEL00P_HOME` + workspace.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};

/// Retry on a transient error: the first hit to the (only) inference slot returns
/// a `429` with `Retry-After: 0`; the same-route retry then returns final text.
/// The default CLI retry policy rides it out, so the run succeeds — and the model
/// is hit twice (the failed attempt + the successful retry).
#[test]
fn transient_429_is_retried_on_the_same_route_and_succeeds() {
    let provider = MockProvider::start().transient_error_then_text(
        429,
        "rate limit exceeded",
        "recovered after a transient rate limit",
    );

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);

    // The default RetryPolicy retried the same route and the turn ultimately
    // succeeded despite the first transient failure.
    result.assert_success();
    result.assert_turn_completed();

    // Exactly two model round-trips: the 429 attempt, then the retry that returned
    // the final text. (Default policy allows up to two retries; only one was
    // needed.)
    assert_eq!(
        provider.hits(),
        2,
        "expected the failed attempt + one successful retry, got {} round-trips",
        provider.hits()
    );

    // The final assistant text is surfaced on stdout.
    assert!(
        result
            .stdout()
            .contains("recovered after a transient rate limit"),
        "final assistant text should appear after the retry, stdout:\n{}",
        result.stdout()
    );
}

/// A `503`-class transient error is likewise retryable: the binary retries the
/// same route and succeeds. This exercises the second retryable family
/// (provider-unavailable) the error classifier recognizes, again via the real
/// CLI inference path with no retry config (defaults only).
#[test]
fn transient_503_is_retried_and_succeeds() {
    let provider = MockProvider::start().transient_error_then_text(
        503,
        "service temporarily unavailable",
        "back online",
    );

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Ping.", "--tool-set", "all"]);

    result.assert_success();
    assert_eq!(
        provider.hits(),
        2,
        "503 should be retried once on the same route, got {} round-trips",
        provider.hits()
    );
    assert!(
        result.stdout().contains("back online"),
        "final text should follow the retry, stdout:\n{}",
        result.stdout()
    );
}

/// A retry that itself runs *inside* a multi-iteration tool loop: a tool-call turn
/// runs first, then the *final* inference slot transiently fails once before
/// succeeding. The tool still runs, the retry recovers, and the loop closes
/// normally — proving retry is transparent to the harness's turn loop.
#[test]
fn retry_is_transparent_inside_the_tool_loop() {
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            serde_json::json!({ "path": "note.txt", "content": "hi\n" }),
        )
        .transient_error_then_text(429, "slow down", "wrote the file");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Make a file.", "--tool-set", "all"]);
    result.assert_success();
    result.assert_tool_called("create_file");
    result.assert_turn_completed();

    // Three round-trips: turn 0 (tool call), the 429 on the final slot, and the
    // retry that returned the final text.
    assert_eq!(
        provider.hits(),
        3,
        "tool turn + failed attempt + retry = 3 round-trips, got {}",
        provider.hits()
    );

    // The side effect happened exactly once (the tool turn was not re-run by the
    // retry, which only re-issues the failed inference).
    assert!(
        runner.workspace_path().join("note.txt").exists(),
        "the tool-created file should exist"
    );
}

/// Model catalog / provider listing: `config providers list` reads the static
/// built-in registry and prints providers with their credential mark, API mode,
/// and streaming capability — fully hermetic (no key, no network). This is a plain
/// (non-`agent`) subcommand, so the runner injects no provider/model/base-url
/// flags.
#[test]
fn providers_list_prints_the_static_catalog() {
    let runner = CodelRunner::new();
    let result = runner.run(&["config", "providers", "list"]);
    result.assert_success();

    let stdout = result.stdout();
    // The header and the per-provider table the command always renders.
    assert!(
        stdout.contains("Providers ([x] = credential available):"),
        "expected the providers list header, stdout:\n{stdout}"
    );
    // A representative built-in provider id and the streaming-capability column.
    assert!(
        stdout.contains("openai"),
        "the built-in catalog should include openai, stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("stream="),
        "each row reports its streaming capability, stdout:\n{stdout}"
    );
    // The actionable hints the command appends.
    assert!(
        stdout.contains("codel00p config providers set-key"),
        "the list should hint how to set a key, stdout:\n{stdout}"
    );
}

/// `config providers show <id>` prints a single provider's details from the static
/// registry: api mode, base url, streaming, env vars, and credential status —
/// hermetic, no network. With no credential in the isolated home, it reports the
/// credential as missing.
#[test]
fn providers_show_prints_provider_details() {
    let runner = CodelRunner::new();
    let result = runner.run(&["config", "providers", "show", "openai"]);
    result.assert_success();

    let stdout = result.stdout();
    assert!(
        stdout.contains("openai"),
        "show should name the provider, stdout:\n{stdout}"
    );
    for field in [
        "api mode:",
        "base url:",
        "streaming:",
        "env vars:",
        "credential:",
    ] {
        assert!(
            stdout.contains(field),
            "show should print the `{field}` field, stdout:\n{stdout}"
        );
    }
    // No credential was seeded into this isolated home for openai.
    assert!(
        stdout.contains("credential:   missing"),
        "with no key, show should report the credential as missing, stdout:\n{stdout}"
    );
}

/// `config providers show <unknown>` fails with a clear error and a non-zero exit,
/// confirming the catalog command validates against the static registry.
#[test]
fn providers_show_rejects_an_unknown_provider() {
    let runner = CodelRunner::new();
    let result = runner.run(&["config", "providers", "show", "definitely-not-a-provider"]);
    assert!(
        !result.success(),
        "an unknown provider should fail, stdout:\n{}\nstderr:\n{}",
        result.stdout(),
        result.stderr()
    );
    assert!(
        result.stderr().contains("unknown provider")
            || result.stdout().contains("unknown provider"),
        "the error should name the unknown provider, stdout:\n{}\nstderr:\n{}",
        result.stdout(),
        result.stderr()
    );
}

/// **Usage IS surfaced through the binary.** A normal `agent run` whose mock
/// response carries an OpenAI-style `usage` block emits protocol events that
/// carry the token usage: the per-inference `InferenceCompleted` event and the
/// turn-total `TurnCompleted` event both expose a `usage` payload. (Supersedes
/// the previous boundary note that documented usage/cost as *not* surfaced —
/// the protocol now mirrors usage on these events; see
/// `codel00p-protocol/src/events.rs::{TokenUsage, CostEstimate}`.)
#[test]
fn usage_is_exposed_in_the_event_stream() {
    // prompt_tokens=120 with no cached-token detail -> input_tokens=120;
    // completion_tokens=34 -> output_tokens=34.
    let provider = MockProvider::start().assistant_text_with_usage("done", 120, 34);
    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Say hi.", "--tool-set", "all"]);
    result.assert_success();

    let events = result.events();
    assert!(!events.is_empty(), "expected emitted protocol events");

    // The per-inference event carries the usage the provider reported.
    let inference_usage = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::InferenceCompleted { usage, .. } => usage.clone(),
            _ => None,
        })
        .expect("InferenceCompleted should carry usage");
    assert_eq!(inference_usage.input_tokens, 120);
    assert_eq!(inference_usage.output_tokens, 34);

    // The terminal event reports the aggregated turn total.
    let turn_usage = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::TurnCompleted { usage, .. } => usage.clone(),
            _ => None,
        })
        .expect("TurnCompleted should carry the aggregated usage");
    assert_eq!(turn_usage.input_tokens, 120);
    assert_eq!(turn_usage.output_tokens, 34);
    assert_eq!(turn_usage.total_tokens(), 154);

    // The serialized stream now contains the usage keys.
    let serialized = events
        .iter()
        .map(|event| serde_json::to_string(event).expect("serialize event"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        serialized.contains("input_tokens") && serialized.contains("output_tokens"),
        "the event stream should carry token usage; events:\n{serialized}"
    );
}

/// Documented boundary: **provider/model fallback routing is not reachable through
/// the real binary.** Fallback is a per-request route chain
/// (`InferenceRequest::fallback_route_with_base_url`) consumed by
/// `InferenceClient::complete`, but there is no CLI flag, config-TOML key, or env
/// var that populates it — `codel00p-cli/src` and `codel00p-harness/src` never call
/// `fallback_route*`, and the harness's provider adapter builds the request without
/// a fallback route. So the binary always runs with an empty fallback chain and the
/// behavior cannot be exercised end-to-end.
///
/// This is asserted at the library layer instead:
/// `codel00p-providers/tests/fallback_routing.rs` proves a rate-limited primary
/// falls back to a healthy secondary and that non-fallbackable errors (e.g. `401`)
/// do not, and `codel00p-providers/tests/retry_backoff.rs` proves retry-then-
/// fallback ordering. This test is `#[ignore]`d because there is no hermetic CLI
/// path to drive; it exists to record the boundary next to the reachable scenarios.
#[test]
#[ignore = "fallback routing has no CLI/config/env surface; covered in codel00p-providers/tests/fallback_routing.rs"]
fn fallback_routing_has_no_cli_surface() {
    // Intentionally empty. See the doc comment: there is no `agent run` flag,
    // config key, or environment variable that adds a fallback route to the
    // inference request the binary builds, so this behavior is not reachable
    // hermetically through the product and is covered by the providers crate.
}
