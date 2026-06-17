use std::{fs, process::Command};

use codel00p_harness::{HarnessError, PermissionScope, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn git_defaults_expose_scoped_workflow_tools() {
    let registry = ToolRegistry::git_defaults();

    assert_eq!(
        registry.names(),
        vec![
            "git_commit",
            "git_diff",
            "git_log",
            "git_status",
            "prepare_pr"
        ]
    );
    assert_eq!(
        registry.permission_scope("git_status", &json!({})),
        PermissionScope::ReadOnly
    );
    assert_eq!(
        registry.permission_scope("git_diff", &json!({})),
        PermissionScope::ReadOnly
    );
    assert_eq!(
        registry.permission_scope("git_log", &json!({})),
        PermissionScope::ReadOnly
    );
    assert_eq!(
        registry.permission_scope("git_commit", &json!({})),
        PermissionScope::WorkspaceWrite
    );
}

#[tokio::test]
async fn git_status_reports_clean_modified_and_untracked_paths() {
    let dir = git_repo();
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    fs::write(dir.path().join("new.txt"), "new\n").expect("write untracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let result = registry
        .execute("git_status", &workspace, json!({}))
        .await
        .expect("git status");

    assert_eq!(result.content()["clean"], false);
    assert_eq!(
        result.content()["files"],
        json!([
            { "path": "new.txt", "index": "untracked", "worktree": "untracked" },
            { "path": "tracked.txt", "index": "unmodified", "worktree": "modified" }
        ])
    );
}

#[tokio::test]
async fn git_diff_reports_tracked_file_diff_with_output_cap() {
    let dir = git_repo();
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let result = registry
        .execute("git_diff", &workspace, json!({ "max_output_bytes": 80 }))
        .await
        .expect("git diff");

    assert_eq!(result.content()["truncated"], true);
    assert!(
        result.content()["diff"]
            .as_str()
            .expect("diff")
            .contains("diff --git a/tracked.txt b/tracked.txt")
    );
}

#[tokio::test]
async fn git_log_reports_recent_commits() {
    let dir = git_repo();
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let result = registry
        .execute("git_log", &workspace, json!({ "limit": 1 }))
        .await
        .expect("git log");

    assert_eq!(result.content()["commits"][0]["subject"], "initial commit");
    assert!(
        result.content()["commits"][0]["sha"]
            .as_str()
            .expect("sha")
            .len()
            >= 7
    );
}

#[tokio::test]
async fn git_commit_rejects_coauthor_trailers() {
    let dir = git_repo();
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let error = registry
        .execute(
            "git_commit",
            &workspace,
            json!({ "message": "change file\n\nCo-authored-by: Someone <s@example.com>" }),
        )
        .await
        .expect_err("coauthor should be rejected");

    assert!(
        matches!(error, HarnessError::InvalidToolInput { name, message } if name == "git_commit" && message.contains("coauthor"))
    );
}

#[tokio::test]
async fn git_commit_rejects_untracked_files() {
    let dir = git_repo();
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    fs::write(dir.path().join("untracked.txt"), "new\n").expect("write untracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let error = registry
        .execute(
            "git_commit",
            &workspace,
            json!({ "message": "change file" }),
        )
        .await
        .expect_err("untracked should be rejected");

    assert!(
        matches!(error, HarnessError::ToolFailed { name, message } if name == "git_commit" && message.contains("untracked files"))
    );
}

#[tokio::test]
async fn git_commit_stages_tracked_changes_and_reports_commit() {
    let dir = git_repo();
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::git_defaults();

    let result = registry
        .execute(
            "git_commit",
            &workspace,
            json!({ "message": "change file" }),
        )
        .await
        .expect("git commit");

    assert_eq!(result.content()["subject"], "change file");
    assert_eq!(result.content()["files_changed"], 1);

    let status = registry
        .execute("git_status", &workspace, json!({}))
        .await
        .expect("git status");
    assert_eq!(status.content()["clean"], true);
}

fn git_repo() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init"]);
    run_git(dir.path(), &["config", "user.name", "codel00p test"]);
    run_git(
        dir.path(),
        &["config", "user.email", "codel00p@example.test"],
    );
    fs::write(dir.path().join("tracked.txt"), "initial\n").expect("write tracked");
    run_git(dir.path(), &["add", "tracked.txt"]);
    run_git(dir.path(), &["commit", "-m", "initial commit"]);
    dir
}

fn run_git(path: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
