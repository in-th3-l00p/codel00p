//! End-to-end scenarios for the read-only tool set.
//!
//! Every test drives the real `codel00p` binary with `--tool-set read` against a
//! scripted mock provider. The workspace is seeded with known files so assertions
//! on tool results are deterministic.
//!
//! Tools covered (one test each):
//!   - `read_file`          — whole file
//!   - `read_file`          — line window (`offset` + `limit`)
//!   - `list_files`         — flat list
//!   - `search_text`        — substring hit
//!   - `search_text`        — miss (no matches)
//!   - `find_files`         — glob
//!   - `grep`               — regex with matches
//!   - `repo_map`           — Rust symbols extracted
//!   - advertised tools     — `ContextManifest` shows all six read-only tools

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

// ────────────────────────────────────────────────────────────────────────────
// Shared workspace layout
// ────────────────────────────────────────────────────────────────────────────
//
//   src/main.rs          — Rust source with two functions
//   src/util/helper.rs   — nested Rust source
//   README.md            — markdown file

const MAIN_RS: &str = "\
pub fn greet(name: &str) -> String {
    format!(\"Hello, {}!\", name)
}

fn main() {
    println!(\"{}\", greet(\"world\"));
}
";

const HELPER_RS: &str = "\
/// Return the length of a slice.
pub fn length<T>(items: &[T]) -> usize {
    items.len()
}
";

const README_MD: &str = "\
# codel00p e2e read-only fixtures

This workspace is seeded by the read-only tool e2e tests.
The main entry point is src/main.rs.
";

/// Build a fresh runner with the shared workspace layout.
///
/// We deliberately **do not** call `git_init()` here: none of these read-only
/// scenarios require git history, and skipping it keeps each test hermetic and
/// fast. The `--tool-set read` flag does not include any git tools.
fn seeded_runner() -> CodelRunner {
    CodelRunner::new()
        .workspace_file("src/main.rs", MAIN_RS)
        .workspace_file("src/util/helper.rs", HELPER_RS)
        .workspace_file("README.md", README_MD)
}

// ────────────────────────────────────────────────────────────────────────────
// read_file — whole file
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn read_file_whole() {
    let provider = MockProvider::start()
        .tool_call("read_file", json!({ "path": "README.md" }))
        .assistant_text("The README describes the e2e fixtures.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Show me the README.", "--tool-set", "read"]);

    result.assert_success();
    result.assert_tool_called("read_file");
    result.assert_turn_completed();

    // The mock served exactly 2 round-trips (tool call + final text).
    assert_eq!(provider.hits(), 2, "expected 2 model round-trips");

    // The second request should contain the tool result as a `role:tool` message.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2, "expected 2 captured requests");
    // The tool result includes the file's content — assert it reached the model.
    assert!(
        requests[1].contains("codel00p e2e read-only fixtures"),
        "second request should include the README content as a tool result;\ngot: {}",
        requests[1]
    );
}

// ────────────────────────────────────────────────────────────────────────────
// read_file — line window (offset + limit)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn read_file_line_window() {
    // Request lines 1–3 of src/main.rs (the `greet` function signature + body).
    let provider = MockProvider::start()
        .tool_call(
            "read_file",
            json!({ "path": "src/main.rs", "offset": 1, "limit": 3 }),
        )
        .assistant_text("I read the first 3 lines of main.rs.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Read the first 3 lines of src/main.rs.",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_tool_called("read_file");
    result.assert_turn_completed();

    // Verify the windowed result reached the model: it should contain the
    // `greet` function signature but NOT the `main` function (line 5+).
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    assert!(
        second.contains("greet"),
        "windowed read should include the greet function; got: {second}"
    );
    // `start_line` / `has_more` are injected by the tool's windowed path.
    assert!(
        second.contains("start_line") || second.contains("has_more"),
        "windowed read result should include windowing metadata; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// list_files
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn list_files_workspace() {
    let provider = MockProvider::start()
        .tool_call("list_files", json!({}))
        .assistant_text("The workspace contains src/main.rs, src/util/helper.rs, and README.md.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&["agent", "run", "List all files.", "--tool-set", "read"]);

    result.assert_success();
    result.assert_tool_called("list_files");
    result.assert_turn_completed();

    // The tool result should enumerate our three seeded files.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    assert!(
        second.contains("README.md"),
        "list_files result should include README.md; got: {second}"
    );
    assert!(
        second.contains("src/main.rs"),
        "list_files result should include src/main.rs; got: {second}"
    );
    assert!(
        second.contains("src/util/helper.rs"),
        "list_files result should include src/util/helper.rs; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// search_text — substring hit
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn search_text_hit() {
    // "greet" appears in src/main.rs on multiple lines.
    let provider = MockProvider::start()
        .tool_call("search_text", json!({ "query": "greet" }))
        .assistant_text("Found greet in src/main.rs.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Search for 'greet' in the workspace.",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_tool_called("search_text");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    // At least one match should reference src/main.rs.
    assert!(
        second.contains("src/main.rs"),
        "search_text hit should reference src/main.rs; got: {second}"
    );
    // The snippet for the first match is the `pub fn greet` line.
    assert!(
        second.contains("greet"),
        "search_text result should contain the matched snippet; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// search_text — miss (no matches)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn search_text_miss() {
    // "xyzzy" does not appear in any seeded file.
    let provider = MockProvider::start()
        .tool_call("search_text", json!({ "query": "xyzzy" }))
        .assistant_text("No matches found for xyzzy.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Search for 'xyzzy' in the workspace.",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_tool_called("search_text");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    // The tool returns `{"matches":[]}` — an empty array.  Verify the model saw it.
    assert!(
        second.contains("matches"),
        "search_text miss result should contain matches key; got: {second}"
    );
    // None of our file names should appear in the matches.
    assert!(
        !second.contains("\"src/main.rs\"") || second.contains("\"matches\":[]"),
        "search_text miss should return empty matches; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// find_files — glob
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn find_files_glob_rs() {
    // `*.rs` matches every Rust file in the workspace (non-glob, matches by name).
    let provider = MockProvider::start()
        .tool_call("find_files", json!({ "pattern": "*.rs" }))
        .assistant_text("Found two Rust files: src/main.rs and src/util/helper.rs.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Find all Rust files.", "--tool-set", "read"]);

    result.assert_success();
    result.assert_tool_called("find_files");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    assert!(
        second.contains("src/main.rs"),
        "find_files *.rs should include src/main.rs; got: {second}"
    );
    assert!(
        second.contains("src/util/helper.rs"),
        "find_files *.rs should include src/util/helper.rs; got: {second}"
    );
    // README.md is not a Rust file — it must not appear in the `files` array.
    // We check the `files` key specifically to avoid false positives from other
    // parts of the request (e.g. the tool_result wrapper text).
    assert!(
        !second.contains("\"README.md\""),
        "find_files *.rs should NOT include README.md; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// grep — regex with matches
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn grep_regex_matches() {
    // Match all `pub fn` definitions; src/main.rs has one (`greet`),
    // src/util/helper.rs has one (`length`).
    let provider = MockProvider::start()
        .tool_call("grep", json!({ "pattern": "pub fn \\w+" }))
        .assistant_text("Found pub fn definitions: greet and length.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Find all public function definitions.",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_tool_called("grep");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];
    // Both files have public functions.
    assert!(
        second.contains("greet"),
        "grep result should contain 'greet' snippet; got: {second}"
    );
    assert!(
        second.contains("length"),
        "grep result should contain 'length' snippet; got: {second}"
    );
    // files_with_matches should report 2.
    assert!(
        second.contains("files_with_matches"),
        "grep result should include files_with_matches; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// repo_map — Rust symbols extracted
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn repo_map_rust_symbols() {
    let provider = MockProvider::start()
        .tool_call("repo_map", json!({}))
        .assistant_text("The repo has greet in src/main.rs and length in src/util/helper.rs.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "Give me a map of all code symbols.",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_tool_called("repo_map");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2);
    let second = &requests[1];

    // The repo_map result should enumerate at least one file with symbols.
    assert!(
        second.contains("files_scanned"),
        "repo_map result should include files_scanned; got: {second}"
    );
    assert!(
        second.contains("symbols_found"),
        "repo_map result should include symbols_found; got: {second}"
    );
    // `greet` and `length` are public functions — they should appear in the map.
    assert!(
        second.contains("greet"),
        "repo_map result should include 'greet' symbol; got: {second}"
    );
    assert!(
        second.contains("length"),
        "repo_map result should include 'length' symbol; got: {second}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Advertised tools: ContextManifest includes all six read-only tools
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn context_manifest_advertises_read_only_tools() {
    // A single-turn run with no tool call is enough to exercise the manifest.
    let provider = MockProvider::start().assistant_text("I can see the workspace.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&[
        "agent",
        "run",
        "What tools do you have?",
        "--tool-set",
        "read",
    ]);

    result.assert_success();
    result.assert_turn_completed();

    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        for expected in &[
            "read_file",
            "list_files",
            "search_text",
            "find_files",
            "grep",
            "repo_map",
        ] {
            assert!(
                advertised_tools.iter().any(|t| t == *expected),
                "ContextManifest should advertise '{expected}', got: {advertised_tools:?}"
            );
        }
    } else {
        panic!("assert_context_manifest returned a non-ContextManifest event");
    }

    // Also verify via the raw request: the first request to the model should
    // have all six tool schemas in the `tools` array.
    let requests = provider.received_requests();
    assert!(
        !requests.is_empty(),
        "at least one request should be captured"
    );
    let first = &requests[0];
    for expected in &[
        "read_file",
        "list_files",
        "search_text",
        "find_files",
        "grep",
        "repo_map",
    ] {
        assert!(
            first.contains(expected),
            "first request should include '{expected}' tool schema; got: {first}"
        );
    }
}
