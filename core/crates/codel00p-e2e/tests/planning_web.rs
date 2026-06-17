//! End-to-end scenarios for the working-plan and web tools.
//!
//! Every test drives the real `codel00p` binary against a scripted mock
//! provider. Two distinct tool families are covered here:
//!
//!   - `update_plan` — the structured working-plan tool. It is added by
//!     `ToolRegistry::planning_defaults()`, which the CLI folds in for *every*
//!     agent regardless of `--tool-set` (see
//!     `codel00p-cli/src/agent/tooling.rs`), so it is reachable under any preset.
//!   - `web_fetch` / `web_search` — the network tools, included by the `web` and
//!     `all` presets (`ToolRegistry::web_defaults()`).
//!
//! # Hermeticity
//!
//! `web_fetch` performs a plain HTTP GET on the model-provided `url`, validating
//! only that it is an `http(s)` URL (`web::validate_http_url`); there is no
//! SSRF/localhost blocking, so we point it at a second local `httpmock` server.
//!
//! `web_search` resolves its backend from the environment at startup
//! (`WebSearchTool::from_env`): it requires `BRAVE_SEARCH_API_KEY` and lets the
//! endpoint be overridden with `CODEL00P_SEARCH_API_URL` (which must speak the
//! Brave web-results JSON shape). Because the endpoint is overridable, the search
//! path is fully hermetic here — we set both env vars and point the endpoint at a
//! local `httpmock` server returning a Brave-shaped payload. The real Brave
//! provider path (no endpoint override, a live key) is covered by an
//! `#[ignore]`-d `[live]` test below.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use httpmock::{Method::GET, MockServer};
use serde_json::json;

// ────────────────────────────────────────────────────────────────────────────
// update_plan — structured step list, one in_progress at a time
// ────────────────────────────────────────────────────────────────────────────

/// The model records a three-step plan (one `in_progress`, respecting the
/// one-at-a-time rule the tool enforces), then finishes. We assert the tool ran
/// and that the echoed plan — which the tool returns to the model — flows back
/// into the next request. The plan lives in an in-memory `PlanStore`, NOT a file
/// under `CODEL00P_HOME`, and the `ToolCallCompleted` event carries only the tool
/// name; so the only end-to-end observable of the plan's *contents* is the tool
/// result echoed into the following model request.
#[test]
fn update_plan_records_structured_steps() {
    let provider = MockProvider::start()
        .tool_call(
            "update_plan",
            json!({
                "plan": [
                    { "step": "Read the failing test", "status": "completed" },
                    { "step": "Implement the fix", "status": "in_progress" },
                    { "step": "Run the suite", "status": "pending" }
                ],
                "explanation": "Working through the bug fix."
            }),
        )
        .assistant_text("Plan recorded; working on the fix now.");

    // `read` preset is enough: planning_defaults is added for every tool set.
    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Plan out fixing this bug.",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_tool_called("update_plan");
    result.assert_turn_completed();

    assert_eq!(provider.hits(), 2, "expected 2 model round-trips");

    // The tool echoes `{ plan, total, completed, explanation }` back to the
    // model; that result is the second request's `role:tool` message.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let echoed = &requests[1];
    assert!(
        echoed.contains("Implement the fix"),
        "echoed plan should include the in-progress step; got: {echoed}"
    );
    assert!(
        echoed.contains("in_progress"),
        "echoed plan should preserve the in_progress status; got: {echoed}"
    );
    // The tool computes `total` and `completed` counts. The tool result is a
    // JSON string nested inside the request body, so its quotes are
    // backslash-escaped on the wire (the literal bytes are `total\":3`). Match
    // that escaped form.
    assert!(
        echoed.contains("total\\\":3"),
        "echoed plan should report total=3; got: {echoed}"
    );
    assert!(
        echoed.contains("completed\\\":1"),
        "echoed plan should report completed=1; got: {echoed}"
    );
}

/// `update_plan` is advertised even under `--tool-set read`, because
/// `planning_defaults` is unconditionally folded into every registry.
#[test]
fn context_manifest_advertises_update_plan() {
    let provider = MockProvider::start().assistant_text("Ready.");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Hello.", "--tool-set", "read"]);

    result.assert_success();
    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        assert!(
            advertised_tools.iter().any(|t| t == "update_plan"),
            "ContextManifest should advertise update_plan, got: {advertised_tools:?}"
        );
    } else {
        panic!("assert_context_manifest returned a non-ContextManifest event");
    }
}

// ────────────────────────────────────────────────────────────────────────────
// web_fetch — GET a known body off a local mock HTTP server
// ────────────────────────────────────────────────────────────────────────────

/// The model fetches a known HTML page from a second local `httpmock` server.
/// We assert the tool completed and that the *stripped* page text flowed back to
/// the model in the next request. `web_fetch` renders HTML to plain text (drops
/// tags / `<script>` / `<style>`), so we assert on the visible prose rather than
/// the markup.
#[test]
fn web_fetch_returns_page_text() {
    let web_server = MockServer::start();
    let page = web_server.mock(|when, then| {
        when.method(GET).path("/page");
        then.status(200).header("content-type", "text/html").body(
            "<html><head><title>Ignored</title>\
                 <script>var secret = 42;</script></head>\
                 <body><h1>Hermetic Heading</h1>\
                 <p>Distinctive body sentinel text.</p></body></html>",
        );
    });

    let url = web_server.url("/page");
    let provider = MockProvider::start()
        .tool_call("web_fetch", json!({ "url": url }))
        .assistant_text("Fetched the page.");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Fetch that page and summarize it.",
        "--tool-set",
        "web",
    ]);

    result.assert_success();
    result.assert_tool_called("web_fetch");
    result.assert_turn_completed();

    page.assert();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let echoed = &requests[1];
    // The visible prose should reach the model as the tool result.
    assert!(
        echoed.contains("Hermetic Heading"),
        "fetched text should include the heading; got: {echoed}"
    );
    assert!(
        echoed.contains("Distinctive body sentinel text."),
        "fetched text should include the body sentinel; got: {echoed}"
    );
    // `<script>` contents are stripped by the HTML-to-text pass.
    assert!(
        !echoed.contains("var secret = 42"),
        "script contents must be stripped from fetched text; got: {echoed}"
    );
}

/// Both web tools are advertised under `--tool-set web`.
#[test]
fn context_manifest_advertises_web_tools() {
    let provider = MockProvider::start().assistant_text("Ready.");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Hello.", "--tool-set", "web"]);

    result.assert_success();
    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        for expected in &["web_fetch", "web_search"] {
            assert!(
                advertised_tools.iter().any(|t| t == *expected),
                "ContextManifest should advertise '{expected}', got: {advertised_tools:?}"
            );
        }
    } else {
        panic!("assert_context_manifest returned a non-ContextManifest event");
    }
}

// ────────────────────────────────────────────────────────────────────────────
// web_search — hermetic, via a Brave-compatible endpoint override
// ────────────────────────────────────────────────────────────────────────────

/// `web_search` is hermetically testable: its backend is resolved from the
/// environment, and `CODEL00P_SEARCH_API_URL` overrides the endpoint while
/// `BRAVE_SEARCH_API_KEY` enables the backend at all. We point the endpoint at a
/// local `httpmock` server returning a Brave-shaped payload (`web.results[]` with
/// `title` / `url` / `description`) and assert the parsed `{ title, url, snippet }`
/// results flow back to the model.
#[test]
fn web_search_returns_results_from_mock_backend() {
    let search_server = MockServer::start();
    let search = search_server.mock(|when, then| {
        when.method(GET)
            .path("/search")
            .query_param("q", "rustlang");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({
                "web": {
                    "results": [
                        {
                            "title": "The Rust Programming Language",
                            "url": "https://www.rust-lang.org",
                            "description": "A language empowering everyone."
                        },
                        {
                            "title": "Rust Documentation",
                            "url": "https://doc.rust-lang.org",
                            "description": "The Rust docs."
                        }
                    ]
                }
            }));
    });

    let provider = MockProvider::start()
        .tool_call(
            "web_search",
            json!({ "query": "rustlang", "max_results": 5 }),
        )
        .assistant_text("Here are the search results.");

    // The binary reads these at startup (`WebSearchTool::from_env`); the runner
    // forwards extra env to the spawned process.
    let runner = CodelRunner::new()
        .with_provider(&provider)
        .env("BRAVE_SEARCH_API_KEY", "test-key")
        .env("CODEL00P_SEARCH_API_URL", search_server.url("/search"));
    let result = runner.run(&[
        "agent",
        "run",
        "Search the web for rustlang.",
        "--tool-set",
        "web",
    ]);

    result.assert_success();
    result.assert_tool_called("web_search");
    result.assert_turn_completed();

    search.assert();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let echoed = &requests[1];
    // The parsed results (title/url/snippet) should reach the model.
    assert!(
        echoed.contains("The Rust Programming Language"),
        "search result title should reach the model; got: {echoed}"
    );
    assert!(
        echoed.contains("https://www.rust-lang.org"),
        "search result url should reach the model; got: {echoed}"
    );
    // `description` is remapped to `snippet` by `parse_brave_results`.
    assert!(
        echoed.contains("snippet"),
        "search result should be shaped with a snippet field; got: {echoed}"
    );
    assert!(
        echoed.contains("A language empowering everyone."),
        "search result snippet should reach the model; got: {echoed}"
    );
}

/// `web_search` against the *real* Brave Search API.
///
/// `[live]` — gated `#[ignore]` so the suite stays hermetic. The default backend
/// (no `CODEL00P_SEARCH_API_URL` override) is the real Brave endpoint
/// (`https://api.search.brave.com/...`), which requires a valid
/// `BRAVE_SEARCH_API_KEY` and live network access. Run explicitly with the key
/// set and `--ignored` to exercise the real provider:
///
/// ```sh
/// BRAVE_SEARCH_API_KEY=<key> \
///   cargo test -p codel00p-e2e --test planning_web -- --ignored web_search_live
/// ```
#[test]
#[ignore = "[live]: requires a real BRAVE_SEARCH_API_KEY and network access"]
fn web_search_live() {
    let key = match std::env::var("BRAVE_SEARCH_API_KEY") {
        Ok(key) if !key.trim().is_empty() => key,
        _ => panic!("BRAVE_SEARCH_API_KEY must be set to run the [live] web_search test"),
    };

    let provider = MockProvider::start()
        .tool_call(
            "web_search",
            json!({ "query": "rust programming language" }),
        )
        .assistant_text("Here are the live search results.");

    // No CODEL00P_SEARCH_API_URL override: hits the real Brave endpoint.
    let runner = CodelRunner::new()
        .with_provider(&provider)
        .env("BRAVE_SEARCH_API_KEY", key);
    let result = runner.run(&[
        "agent",
        "run",
        "Search the web for the rust programming language.",
        "--tool-set",
        "web",
    ]);

    result.assert_success();
    result.assert_tool_called("web_search");
    result.assert_turn_completed();
}
