//! Tests for opt-in git-worktree isolation of mutating sub-agents.
//!
//! When a delegated task requests `TaskIsolation::Worktree`, the child runs in
//! its own temporary worktree created off the parent repo's HEAD: its file
//! mutations never touch the parent working tree, and the spawner captures the
//! child's full diff and returns it to the parent.

mod support;

use std::{fs, process::Command, sync::Arc};

use codel00p_harness::{
    AgentRole, DelegatedTask, HarnessInferenceResponse, HarnessSubAgentSpawner, ModelToolCall,
    SubAgentSpawner, TaskIsolation, ToolRegistry, Workspace, delegation_tools,
};
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

/// A git repo fixture with an initial commit on `main` (CI's git defaults to
/// `master`, so the branch is pinned explicitly).
fn git_repo() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init", "-b", "main"]);
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
    let status = Command::new("git")
        .args(args)
        .current_dir(path)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A child that creates a file then summarizes, scripted across two turns.
fn file_creating_child() -> ScriptedModelClient {
    ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "c1",
                "create_file",
                json!({ "path": "new_file.txt", "content": "hello from child\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "created new_file.txt"),
    ])
}

#[tokio::test]
async fn worktree_isolation_mutates_in_a_worktree_and_returns_diff() {
    let repo = git_repo();
    let workspace = Workspace::new(repo.path()).expect("workspace");

    let spawner = HarnessSubAgentSpawner::new(
        Arc::new(file_creating_child()),
        workspace,
        ToolRegistry::editing_defaults(),
    );

    let outcome = spawner
        .spawn(DelegatedTask::new("create a file").with_isolation(TaskIsolation::Worktree))
        .await
        .expect("spawn isolated child");

    // The child's diff flows back to the parent and contains the new file.
    let diff = outcome.diff().expect("worktree isolation yields a diff");
    assert!(
        diff.contains("new_file.txt"),
        "diff should mention the new file, got: {diff}"
    );
    assert!(
        diff.contains("hello from child"),
        "diff should include the new content, got: {diff}"
    );

    // The PARENT working tree is untouched: no new file leaked in.
    assert!(
        !repo.path().join("new_file.txt").exists(),
        "parent working tree must not contain the child's new file"
    );

    // The temporary worktree was cleaned up (no stray worktrees registered).
    let worktrees = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo.path())
        .output()
        .expect("worktree list");
    let listed = String::from_utf8_lossy(&worktrees.stdout);
    // Only the main worktree (the repo root) should remain.
    assert_eq!(
        listed.matches("worktree ").count(),
        1,
        "temporary worktree should have been removed, got: {listed}"
    );
}

#[tokio::test]
async fn shared_isolation_is_unchanged_and_has_no_diff() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let child_model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "shared summary",
    )]);

    let spawner = HarnessSubAgentSpawner::new(
        Arc::new(child_model),
        workspace,
        ToolRegistry::read_only_defaults(),
    );

    let outcome = spawner
        .spawn(DelegatedTask::new("summarize"))
        .await
        .expect("spawn shared child");

    assert_eq!(outcome.summary(), "shared summary");
    assert_eq!(outcome.diff(), None);
}

#[tokio::test]
async fn worktree_isolation_requires_a_git_repo() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let child_model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "never runs",
    )]);

    let spawner = HarnessSubAgentSpawner::new(
        Arc::new(child_model),
        workspace,
        ToolRegistry::editing_defaults(),
    );

    let error = spawner
        .spawn(DelegatedTask::new("create a file").with_isolation(TaskIsolation::Worktree))
        .await
        .expect_err("worktree isolation on a non-git workspace should error");
    let message = error.to_string().to_ascii_lowercase();
    assert!(
        message.contains("git"),
        "error should explain the git-repo requirement, got: {message}"
    );
}

#[tokio::test]
async fn delegate_tool_surfaces_diff_for_worktree_isolation() {
    let repo = git_repo();
    let workspace = Workspace::new(repo.path()).expect("workspace");

    let spawner = Arc::new(HarnessSubAgentSpawner::new(
        Arc::new(file_creating_child()),
        workspace.clone(),
        ToolRegistry::editing_defaults(),
    ));

    let registry = delegation_tools(AgentRole::Orchestrator, spawner);
    let result = registry
        .execute(
            "delegate_task",
            &workspace,
            json!({ "task": "create a file", "isolation": "worktree" }),
        )
        .await
        .expect("delegate with worktree isolation");

    let diff = result.content()["diff"]
        .as_str()
        .expect("delegate_task surfaces a diff for worktree isolation");
    assert!(
        diff.contains("new_file.txt"),
        "surfaced diff should mention the new file, got: {diff}"
    );
}
