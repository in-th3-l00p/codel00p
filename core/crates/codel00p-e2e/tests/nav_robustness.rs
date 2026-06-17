//! End-to-end regression for the `list_files`/`search_text` directory-walk
//! permission/ignore robustness bug fixed in PR #61.
//!
//! Before the fix, both tools used a private recursive `collect_files` that was
//! (a) ignore-unaware — it descended into `target/`, `.git/`, and
//! `node_modules/` and flooded the listing — and (b) fragile — it
//! `?`-propagated any io error, so a single unreadable (EACCES/EPERM) nested
//! directory aborted the *entire* tool call. On macOS this triggers easily via
//! TCC-protected directories. The fix migrates both tools onto the shared,
//! ignore-aware `walk_files`, which now skips unreadable nested directories and
//! individual unreadable entries and keeps going.
//!
//! These scenarios drive the **real** `codel00p` binary with `--tool-set read`
//! against a scripted mock provider, asserting the observable CLI behavior:
//!   - the run SUCCEEDS (does not error/abort),
//!   - the readable files ARE returned,
//!   - the ignored dirs (`target/`, `.git/`, `node_modules/`) are NOT listed,
//!   - an unreadable `0o000` nested dir does not crash the tool (unix-gated).

use codel00p_e2e::{CodelRunner, MockProvider};
use serde_json::json;

// ────────────────────────────────────────────────────────────────────────────
// Shared workspace layout
// ────────────────────────────────────────────────────────────────────────────
//
//   src/main.rs                  — readable top-level source (contains "needle")
//   src/util/helper.rs           — readable nested source (contains "needle")
//   target/debug/build.rs        — MUST be ignored (build artifacts)
//   .git/config                  — MUST be ignored (vcs metadata; seeded as a
//                                  plain dir, NOT a real repo — we don't git_init)
//   node_modules/pkg/index.js    — MUST be ignored (deps)

const MAIN_RS: &str = "\
pub fn greet(name: &str) -> String {
    // needle lives here
    format!(\"Hello, {}!\", name)
}
";

const HELPER_RS: &str = "\
/// needle helper.
pub fn length<T>(items: &[T]) -> usize {
    items.len()
}
";

/// Build a runner seeded with readable sources plus the three ignored dirs.
///
/// We deliberately do **not** call `git_init()`: we want `.git/` to be an inert
/// seeded directory so the ignore-awareness (not git itself) is what excludes
/// it, matching how the harness unit tests exercise the same path.
fn seeded_runner() -> CodelRunner {
    CodelRunner::new()
        .workspace_file("src/main.rs", MAIN_RS)
        .workspace_file("src/util/helper.rs", HELPER_RS)
        .workspace_file("target/debug/build.rs", "// generated needle")
        .workspace_file(".git/config", "[core]\n")
        .workspace_file("node_modules/pkg/index.js", "var needle = 1;")
}

/// The ignored-directory prefixes that must never appear in a listing/match.
/// We assert against quoted, slash-suffixed prefixes so we only catch real
/// `files`/`matches` entries (relative paths) and not, e.g., an ignore-rule
/// string the model was shown elsewhere in the request body.
const IGNORED_PREFIXES: &[&str] = &["\"target/", "\".git/", "\"node_modules/"];

// ────────────────────────────────────────────────────────────────────────────
// list_files — ignore-aware: readable files in, ignored dirs out
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn list_files_excludes_ignored_dirs() {
    let provider = MockProvider::start()
        .tool_call("list_files", json!({}))
        .assistant_text("Listed the source files.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&["agent", "run", "List all files.", "--tool-set", "read"]);

    // (a) The run succeeds and the tool completed without aborting.
    result.assert_success();
    result.assert_tool_called("list_files");
    result.assert_turn_completed();

    // The tool result is delivered to the model as the second request's
    // `role:tool` message — assert on its observed contents.
    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2, "expected 2 model round-trips");
    let second = &requests[1];

    // (b) The readable files ARE returned.
    assert!(
        second.contains("src/main.rs"),
        "list_files should include src/main.rs; got: {second}"
    );
    assert!(
        second.contains("src/util/helper.rs"),
        "list_files should include src/util/helper.rs; got: {second}"
    );

    // (c) The ignored dirs are NOT in the listing.
    for prefix in IGNORED_PREFIXES {
        assert!(
            !second.contains(prefix),
            "list_files should NOT include the ignored entry {prefix}; got: {second}"
        );
    }
}

// ────────────────────────────────────────────────────────────────────────────
// search_text — ignore-aware: readable hits in, ignored dirs out
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn search_text_excludes_ignored_dirs() {
    // "needle" appears in src/main.rs, src/util/helper.rs, AND in each ignored
    // dir's file — but only the readable, non-ignored hits must come back.
    let provider = MockProvider::start()
        .tool_call("search_text", json!({ "query": "needle" }))
        .assistant_text("Searched for needle.");

    let runner = seeded_runner().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Search for 'needle'.", "--tool-set", "read"]);

    result.assert_success();
    result.assert_tool_called("search_text");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2, "expected 2 model round-trips");
    let second = &requests[1];

    // (b) The readable source matches ARE returned.
    assert!(
        second.contains("src/main.rs"),
        "search_text should match src/main.rs; got: {second}"
    );
    assert!(
        second.contains("src/util/helper.rs"),
        "search_text should match src/util/helper.rs; got: {second}"
    );

    // (c) The ignored dirs are NOT among the matches.
    for prefix in IGNORED_PREFIXES {
        assert!(
            !second.contains(prefix),
            "search_text should NOT match the ignored entry {prefix}; got: {second}"
        );
    }
}

// ────────────────────────────────────────────────────────────────────────────
// list_files — tolerates an unreadable (0o000) nested directory (unix-only)
// ────────────────────────────────────────────────────────────────────────────

/// Regression for the macOS-TCC / EPERM bug: a single unreadable nested
/// directory must not abort the whole walk. Gated to unix because it relies on
/// POSIX permission bits.
#[cfg(unix)]
#[test]
fn list_files_tolerates_unreadable_subdir() {
    use std::os::unix::fs::PermissionsExt;

    // A readable top-level file alongside a nested dir we will lock to 0o000.
    let runner = CodelRunner::new()
        .workspace_file("readable.txt", "hello")
        .workspace_file("locked/secret.txt", "nope");

    let locked = runner.workspace_path().join("locked");

    // Restore permissions on drop so the tempdir can be cleaned up even if an
    // assertion below panics (a 0o000 dir is otherwise un-removable).
    struct Restore(std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let _restore = Restore(locked.clone());

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
        .expect("lock the nested dir to 0o000");

    let provider = MockProvider::start()
        .tool_call("list_files", json!({}))
        .assistant_text("Listed the readable files.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "List all files.", "--tool-set", "read"]);

    // (a) + (d) The run SUCCEEDS — the unreadable dir did not crash the tool.
    result.assert_success();
    result.assert_tool_called("list_files");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2, "expected 2 model round-trips");
    let second = &requests[1];

    // (b) The readable file IS still returned despite the locked sibling dir.
    assert!(
        second.contains("readable.txt"),
        "list_files should still return readable.txt past the locked dir; got: {second}"
    );

    // Restore explicitly before the runner's tempdir is dropped (the Drop guard
    // is a backstop for the panic path).
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("restore the locked dir permissions");
}

// ────────────────────────────────────────────────────────────────────────────
// search_text — tolerates an unreadable (0o000) nested directory (unix-only)
// ────────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn search_text_tolerates_unreadable_subdir() {
    use std::os::unix::fs::PermissionsExt;

    let runner = CodelRunner::new()
        .workspace_file("readable.txt", "needle here")
        .workspace_file("locked/secret.txt", "needle hidden");

    let locked = runner.workspace_path().join("locked");

    struct Restore(std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let _restore = Restore(locked.clone());

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
        .expect("lock the nested dir to 0o000");

    let provider = MockProvider::start()
        .tool_call("search_text", json!({ "query": "needle" }))
        .assistant_text("Searched for needle.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Search for 'needle'.", "--tool-set", "read"]);

    // (a) + (d) The run SUCCEEDS — the locked dir did not abort the search.
    result.assert_success();
    result.assert_tool_called("search_text");
    result.assert_turn_completed();

    let requests = provider.received_requests();
    assert_eq!(requests.len(), 2, "expected 2 model round-trips");
    let second = &requests[1];

    // (b) The readable file's match IS returned despite the locked sibling.
    assert!(
        second.contains("readable.txt"),
        "search_text should still match readable.txt past the locked dir; got: {second}"
    );

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("restore the locked dir permissions");
}
