//! End-to-end tests for the git tool group.
//!
//! Each scenario drives the real `codel00p` binary against a scripted mock
//! provider with `--tool-set git` (plus `--tool-set command` where staging
//! untracked files is needed).  Every test uses its own hermetic workspace and
//! `CODEL00P_HOME`.
//!
//! Pattern (mirrors `pilot.rs`):
//!   1. Seed files BEFORE `git_init()` so the initial commit captures them and
//!      the tree is clean — `git_commit` refuses untracked files.
//!   2. Script the mock provider with the exact tool calls the agent should make.
//!   3. Assert on protocol events emitted by the binary AND on real git state.

use std::process::Command;

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns `git log --pretty=format:%s` subjects, newest first.
fn git_log_subjects(repo: &std::path::Path) -> Vec<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["log", "--pretty=format:%s"])
        .output()
        .expect("run git log");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Returns the number of commits reachable from HEAD.
fn git_commit_count(repo: &std::path::Path) -> usize {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .expect("run git rev-list");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<usize>()
        .unwrap_or(0)
}

// ── git_status: clean tree ─────────────────────────────────────────────────

#[test]
fn git_status_clean_tree_reports_clean() {
    // Seed a file before git_init so the initial commit captures it.
    let runner = CodelRunner::new()
        .workspace_file("src/main.rs", "fn main() {}\n")
        .git_init();

    let provider = MockProvider::start()
        .tool_call("git_status", json!({}))
        .assistant_text("Tree is clean.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Check git status.", "--tool-set", "git"]);
    result.assert_success();
    result.assert_tool_called("git_status");
    result.assert_turn_completed();

    // The tool result is fed back to the model as a tool-role message.
    // The content field is a JSON-encoded string, so the result appears as
    // escaped JSON inside the outer JSON: \"clean\":true.
    let requests = provider.received_requests();
    let combined = requests.join("\n");
    assert!(
        combined.contains(r#"\"clean\":true"#),
        "expected clean:true in model context; got:\n{combined}"
    );
}

// ── git_status: dirty tree shows modified file ─────────────────────────────

#[test]
fn git_status_dirty_tree_reports_modified_file() {
    let runner = CodelRunner::new()
        .workspace_file("notes.txt", "initial\n")
        .git_init();

    // Modify the tracked file directly so git sees it as modified.
    std::fs::write(runner.workspace_path().join("notes.txt"), "modified\n")
        .expect("modify tracked file");

    let provider = MockProvider::start()
        .tool_call("git_status", json!({}))
        .assistant_text("I can see the modified file.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "What changed?", "--tool-set", "git"]);
    result.assert_success();
    result.assert_tool_called("git_status");

    // The content field is JSON-encoded so the result appears with escaped
    // quotes inside the outer JSON body: \"clean\":false.
    let requests = provider.received_requests();
    let combined = requests.join("\n");
    assert!(
        combined.contains(r#"\"clean\":false"#),
        "expected clean:false in model context; got:\n{combined}"
    );
    assert!(
        combined.contains("notes.txt"),
        "expected notes.txt in status output; got:\n{combined}"
    );
}

// ── git_diff: reflects tracked modification ────────────────────────────────

#[test]
fn git_diff_reflects_tracked_modification() {
    let runner = CodelRunner::new()
        .workspace_file("data.txt", "line one\n")
        .git_init();

    std::fs::write(
        runner.workspace_path().join("data.txt"),
        "line one\nline two\n",
    )
    .expect("append line to tracked file");

    let provider = MockProvider::start()
        .tool_call("git_diff", json!({}))
        .assistant_text("Diff looks good.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Show the diff.", "--tool-set", "git"]);
    result.assert_success();
    result.assert_tool_called("git_diff");

    let requests = provider.received_requests();
    let combined = requests.join("\n");
    assert!(
        combined.contains("line two"),
        "expected 'line two' in diff output; got:\n{combined}"
    );
    assert!(
        combined.contains("data.txt"),
        "expected data.txt in diff output; got:\n{combined}"
    );
}

// ── git_log: lists seeded commits ──────────────────────────────────────────

#[test]
fn git_log_lists_seeded_initial_commit() {
    let runner = CodelRunner::new()
        .workspace_file("README.md", "# project\n")
        .git_init(); // creates "chore: initial commit"

    let provider = MockProvider::start()
        .tool_call("git_log", json!({}))
        .assistant_text("I see the initial commit.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Show git log.", "--tool-set", "git"]);
    result.assert_success();
    result.assert_tool_called("git_log");

    // Tool result content is JSON-encoded inside the outer request body, so
    // field names appear with escaped quotes: \"subject\", \"sha\".
    let requests = provider.received_requests();
    let combined = requests.join("\n");
    assert!(
        combined.contains("initial commit"),
        "expected 'initial commit' in git_log output; got:\n{combined}"
    );
    assert!(
        combined.contains(r#"\"subject\""#),
        "expected subject field in log; got:\n{combined}"
    );
    assert!(
        combined.contains(r#"\"sha\""#),
        "expected sha field in log; got:\n{combined}"
    );
}

// ── git_commit: happy path on tracked modification ─────────────────────────

#[test]
fn git_commit_happy_path_creates_new_commit() {
    let runner = CodelRunner::new()
        .workspace_file("app.rs", "fn main() {}\n")
        .git_init();

    // Modify the tracked file before running the agent.
    std::fs::write(
        runner.workspace_path().join("app.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .expect("modify tracked file");

    let provider = MockProvider::start()
        .tool_call("git_commit", json!({ "message": "feat: update greeting" }))
        .assistant_text("Commit created.");

    let runner = runner.with_provider(&provider);
    let before = git_commit_count(runner.workspace_path());

    let result = runner.run(&["agent", "run", "Commit the changes.", "--tool-set", "git"]);
    result.assert_success();
    result.assert_tool_called("git_commit");
    result.assert_turn_completed();

    // Verify a new commit was actually created in git history.
    let after = git_commit_count(runner.workspace_path());
    assert_eq!(
        after,
        before + 1,
        "expected one new commit; before={before} after={after}"
    );

    let subjects = git_log_subjects(runner.workspace_path());
    assert!(
        subjects.iter().any(|s| s == "feat: update greeting"),
        "expected 'feat: update greeting' in git log; got {subjects:?}"
    );
}

// ── git_commit: refusal when untracked files are present ───────────────────
//
// git_commit runs `git add -u` (tracked only) and REFUSES to proceed if any
// UNTRACKED files are present (see harness/src/git.rs).  We confirm the tool
// emits a `ToolCallFailed` event rather than creating a commit.

#[test]
fn git_commit_refuses_when_untracked_file_present() {
    let runner = CodelRunner::new()
        .workspace_file("existing.txt", "tracked\n")
        .git_init();

    // Drop an untracked file into the workspace WITHOUT staging it.
    std::fs::write(
        runner.workspace_path().join("untracked_secret.txt"),
        "not staged\n",
    )
    .expect("create untracked file");

    // Also modify the tracked file so the tool doesn't bail on "no changes".
    std::fs::write(runner.workspace_path().join("existing.txt"), "modified\n")
        .expect("modify tracked file");

    // Script: the mock asks the agent to commit; the tool should refuse.
    // After the failure the model receives the error result and replies with text.
    let provider = MockProvider::start()
        .tool_call(
            "git_commit",
            json!({ "message": "feat: should be refused" }),
        )
        .assistant_text("The commit was refused as expected.");

    let runner = runner.with_provider(&provider);
    let before = git_commit_count(runner.workspace_path());

    let result = runner.run(&["agent", "run", "Commit everything.", "--tool-set", "git"]);
    // The binary itself should still exit 0 (tool error is reported to the
    // model, not as a fatal process exit).
    result.assert_success();

    // A ToolCallFailed event must have been emitted for git_commit.
    assert!(
        result.has_event(|event| matches!(
            event,
            AgentEvent::ToolCallFailed { tool_name, message, .. }
                if tool_name == "git_commit"
                    && message.contains("untracked")
        )),
        "expected ToolCallFailed for git_commit mentioning 'untracked'; events:\n{:#?}",
        result.events()
    );

    // No new commit should have been created.
    let after = git_commit_count(runner.workspace_path());
    assert_eq!(
        after, before,
        "git_commit should not have created a commit when untracked files are present"
    );

    // The error result was fed back to the model.
    let requests = provider.received_requests();
    let combined = requests.join("\n");
    assert!(
        combined.contains("untracked"),
        "expected the refusal message ('untracked') to appear in model context; got:\n{combined}"
    );
}

// ── prepare_pr: branch ahead of main returns structured summary ────────────

#[test]
fn prepare_pr_branch_ahead_returns_structured_summary() {
    // Build a workspace with an initial commit on main and a feature branch
    // that has one additional commit.  We do this entirely via the CodelRunner
    // helpers and direct git commands; the agent only calls prepare_pr.
    let runner = CodelRunner::new()
        .workspace_file("base.txt", "base content\n")
        .git_init(); // -> main branch, "chore: initial commit"

    let ws = runner.workspace_path();

    // Create a feature branch with one commit.
    let run_git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(ws)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("git {}: {e}", args.join(" ")));
        assert!(
            out.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run_git(&["checkout", "-b", "feat/add-feature"]);
    std::fs::write(ws.join("feature.txt"), "new feature\n").expect("write feature.txt");
    run_git(&["add", "feature.txt"]);
    run_git(&["commit", "-m", "feat: add feature file"]);

    let provider = MockProvider::start()
        .tool_call("prepare_pr", json!({ "base": "main" }))
        .assistant_text("PR summary ready.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Summarise the PR.", "--tool-set", "git"]);
    result.assert_success();
    result.assert_tool_called("prepare_pr");
    result.assert_turn_completed();

    // The prepare_pr result is returned to the model as a tool message.
    // The content field is JSON-encoded, so field names appear with escaped
    // quotes inside the outer request body: \"field_name\".
    let requests = provider.received_requests();
    let combined = requests.join("\n");

    // commits: at least one commit returned
    assert!(
        combined.contains(r#"\"commits\""#),
        "expected 'commits' field in prepare_pr result; got:\n{combined}"
    );
    assert!(
        combined.contains("feat: add feature file"),
        "expected feature commit subject in prepare_pr result; got:\n{combined}"
    );

    // diffstat present
    assert!(
        combined.contains(r#"\"diffstat\""#),
        "expected 'diffstat' field in prepare_pr result; got:\n{combined}"
    );

    // draft_body present with ## Summary section (the value is a JSON string;
    // the ## characters are not special and appear literally).
    assert!(
        combined.contains("## Summary"),
        "expected '## Summary' in draft_body; got:\n{combined}"
    );

    // title_suggestion present
    assert!(
        combined.contains(r#"\"title_suggestion\""#),
        "expected 'title_suggestion' field in prepare_pr result; got:\n{combined}"
    );

    // pr_ready should be true (1 commit, 1 file changed)
    assert!(
        combined.contains(r#"\"pr_ready\":true"#),
        "expected pr_ready:true in prepare_pr result; got:\n{combined}"
    );
}
