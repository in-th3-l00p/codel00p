use std::{fs, process::Command};

use codel00p_harness::{PermissionScope, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

// ── helpers ──────────────────────────────────────────────────────────────────

fn git_repo_with_branch() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.name", "codel00p test"]);
    run_git(
        dir.path(),
        &["config", "user.email", "codel00p@example.test"],
    );

    // Initial commit on main (base)
    fs::write(dir.path().join("readme.txt"), "hello\n").expect("write readme");
    run_git(dir.path(), &["add", "readme.txt"]);
    run_git_env(
        dir.path(),
        &[
            "commit",
            "-m",
            "initial commit",
            "--date=2025-01-01T00:00:00+00:00",
        ],
        &[("GIT_COMMITTER_DATE", "2025-01-01T00:00:00+00:00")],
    );

    // Create feature branch
    run_git(dir.path(), &["checkout", "-b", "feature/add-stuff"]);

    // First commit on branch
    fs::write(dir.path().join("alpha.txt"), "alpha content\n").expect("write alpha");
    run_git(dir.path(), &["add", "alpha.txt"]);
    run_git_env(
        dir.path(),
        &[
            "commit",
            "-m",
            "add alpha file",
            "--date=2025-01-02T00:00:00+00:00",
        ],
        &[("GIT_COMMITTER_DATE", "2025-01-02T00:00:00+00:00")],
    );

    // Second commit on branch
    fs::write(dir.path().join("beta.txt"), "beta content\n").expect("write beta");
    run_git(dir.path(), &["add", "beta.txt"]);
    run_git_env(
        dir.path(),
        &[
            "commit",
            "-m",
            "add beta file",
            "--date=2025-01-03T00:00:00+00:00",
        ],
        &[("GIT_COMMITTER_DATE", "2025-01-03T00:00:00+00:00")],
    );

    dir
}

fn run_git(path: &std::path::Path, args: &[&str]) {
    run_git_env(path, args, &[]);
}

fn run_git_env(path: &std::path::Path, args: &[&str], env: &[(&str, &str)]) {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(path);
    for (key, val) in env {
        cmd.env(key, val);
    }
    let output = cmd.output().expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn prepare_pr_is_in_git_defaults() {
    let registry = ToolRegistry::git_defaults();
    assert!(
        registry.names().contains(&"prepare_pr".to_string()),
        "prepare_pr must be in git_defaults(); got: {:?}",
        registry.names()
    );
    assert_eq!(
        registry.permission_scope("prepare_pr", &json!({})),
        PermissionScope::ReadOnly
    );
}

#[tokio::test]
async fn prepare_pr_multi_commit_branch_returns_correct_structure() {
    let dir = git_repo_with_branch();
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    // We're on feature/add-stuff, base = main
    let result = registry
        .execute("prepare_pr", &workspace, json!({ "base": "main" }))
        .await
        .expect("prepare_pr");

    let content = result.content();

    // Should be ready
    assert_eq!(content["pr_ready"], true);

    // base + head populated
    assert_eq!(content["base"], "main");
    assert!(
        content["head"].as_str().is_some(),
        "head should be a string"
    );

    // commits: 2 commits on the branch
    let commits = content["commits"].as_array().expect("commits array");
    assert_eq!(commits.len(), 2, "expected 2 commits; got {commits:?}");

    let subjects: Vec<&str> = commits
        .iter()
        .map(|c| c["subject"].as_str().expect("subject"))
        .collect();
    // git log is newest-first
    assert!(
        subjects.contains(&"add alpha file"),
        "missing 'add alpha file' in {subjects:?}"
    );
    assert!(
        subjects.contains(&"add beta file"),
        "missing 'add beta file' in {subjects:?}"
    );

    // Each commit has sha, author_name, author_email, subject
    for commit in commits {
        assert!(commit["sha"].as_str().is_some());
        assert!(commit["author_name"].as_str().is_some());
        assert!(commit["author_email"].as_str().is_some());
        assert!(commit["subject"].as_str().is_some());
    }

    // files_changed: 2 new files
    let files = content["files_changed"].as_array().expect("files_changed");
    assert_eq!(files.len(), 2, "expected 2 changed files; got {files:?}");
    let paths: Vec<&str> = files
        .iter()
        .map(|f| f["path"].as_str().expect("path"))
        .collect();
    assert!(
        paths.contains(&"alpha.txt"),
        "missing alpha.txt in {paths:?}"
    );
    assert!(paths.contains(&"beta.txt"), "missing beta.txt in {paths:?}");

    // diffstat totals
    let diffstat = &content["diffstat"];
    assert_eq!(diffstat["files"], 2);
    assert!(
        diffstat["additions"].as_u64().unwrap_or(0) >= 2,
        "expected >=2 additions"
    );

    // diff_preview is present
    assert!(content["diff_preview"].as_str().is_some());
    assert!(content["truncated"].is_boolean());

    // draft_body contains the commit subjects
    let draft = content["draft_body"].as_str().expect("draft_body");
    assert!(draft.contains("add alpha file"), "draft_body: {draft}");
    assert!(draft.contains("add beta file"), "draft_body: {draft}");
    assert!(
        draft.contains("## Summary"),
        "draft_body missing ## Summary"
    );
    assert!(
        draft.contains("## Files changed"),
        "draft_body missing ## Files changed"
    );

    // title_suggestion present (non-empty)
    let title = content["title_suggestion"]
        .as_str()
        .expect("title_suggestion");
    assert!(!title.is_empty(), "title_suggestion should not be empty");
}

#[tokio::test]
async fn prepare_pr_single_commit_uses_subject_as_title() {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.name", "codel00p test"]);
    run_git(
        dir.path(),
        &["config", "user.email", "codel00p@example.test"],
    );

    // Base commit on main
    fs::write(dir.path().join("readme.txt"), "hello\n").expect("write readme");
    run_git(dir.path(), &["add", "readme.txt"]);
    run_git(dir.path(), &["commit", "-m", "initial commit"]);

    // Single commit on feature branch
    run_git(dir.path(), &["checkout", "-b", "feature/single"]);
    fs::write(dir.path().join("solo.txt"), "solo\n").expect("write solo");
    run_git(dir.path(), &["add", "solo.txt"]);
    run_git(
        dir.path(),
        &["commit", "-m", "add solo feature for testing"],
    );

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let result = registry
        .execute("prepare_pr", &workspace, json!({ "base": "main" }))
        .await
        .expect("prepare_pr single");

    let content = result.content();
    assert_eq!(content["pr_ready"], true);
    assert_eq!(content["commits"].as_array().unwrap().len(), 1);

    // With exactly one commit, title_suggestion should equal the commit subject
    let title = content["title_suggestion"]
        .as_str()
        .expect("title_suggestion");
    assert_eq!(
        title, "add solo feature for testing",
        "single-commit title should be the commit subject"
    );
}

#[tokio::test]
async fn prepare_pr_empty_range_returns_pr_ready_false() {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "-b", "main"]);
    run_git(dir.path(), &["config", "user.name", "codel00p test"]);
    run_git(
        dir.path(),
        &["config", "user.email", "codel00p@example.test"],
    );
    fs::write(dir.path().join("readme.txt"), "hello\n").expect("write readme");
    run_git(dir.path(), &["add", "readme.txt"]);
    run_git(dir.path(), &["commit", "-m", "initial commit"]);

    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    // base == head → empty range
    let result = registry
        .execute(
            "prepare_pr",
            &workspace,
            json!({ "base": "main", "head": "main" }),
        )
        .await
        .expect("prepare_pr empty");

    let content = result.content();
    assert_eq!(
        content["pr_ready"], false,
        "empty range should not be ready"
    );
    assert_eq!(
        content["commits"].as_array().unwrap().len(),
        0,
        "no commits expected"
    );
    assert_eq!(
        content["files_changed"].as_array().unwrap().len(),
        0,
        "no files expected"
    );
    // diffstat zeros
    assert_eq!(content["diffstat"]["files"], 0);
    assert_eq!(content["diffstat"]["additions"], 0);
    assert_eq!(content["diffstat"]["deletions"], 0);
}

#[tokio::test]
async fn prepare_pr_diff_preview_truncated_when_capped() {
    let dir = git_repo_with_branch();
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    // Use a tiny cap to force truncation
    let result = registry
        .execute(
            "prepare_pr",
            &workspace,
            json!({ "base": "main", "max_diff_bytes": 10 }),
        )
        .await
        .expect("prepare_pr capped");

    let content = result.content();
    assert_eq!(content["truncated"], true);
    let preview = content["diff_preview"].as_str().expect("diff_preview");
    assert!(
        preview.len() <= 10,
        "preview should be capped to 10 bytes, got {}",
        preview.len()
    );
}
