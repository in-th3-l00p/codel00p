use std::process::Command;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, parse_verbosity, required_string, verbosity_schema},
    workspace::Workspace,
};

pub(crate) const DEFAULT_DIFF_BYTES: usize = 16_384;
const MAX_DIFF_BYTES: usize = 131_072;
const DEFAULT_LOG_LIMIT: usize = 10;
const MAX_LOG_LIMIT: usize = 100;

pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Show stable porcelain git status for the workspace."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let status = git_output(self.name(), workspace, &["status", "--porcelain=v1"])?;
        let mut files = parse_status(&status);
        files.sort_by(|left, right| {
            left["path"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["path"].as_str().unwrap_or_default())
        });

        Ok(ToolResult::json(json!({
            "clean": files.is_empty(),
            "files": files,
        })))
    }
}

pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show git diff output for tracked workspace changes. `verbosity` controls \
         the shape: \"detailed\" (default) returns the full unified diff; \
         \"concise\" returns the `--stat` summary (changed files with insertion/ \
         deletion counts) instead, for a cheap overview of what changed."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_output_bytes": { "type": "integer" },
                "verbosity": verbosity_schema()
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let max_output_bytes =
            optional_usize(&input, "max_output_bytes", DEFAULT_DIFF_BYTES).min(MAX_DIFF_BYTES);
        let verbosity = parse_verbosity(self.name(), &input)?;

        // Concise asks git for the `--stat` summary; detailed (default) returns
        // the full unified diff, byte-identical to the historical output.
        let args: &[&str] = if verbosity.is_concise() {
            &["diff", "--no-ext-diff", "--stat"]
        } else {
            &["diff", "--no-ext-diff"]
        };
        let diff = git_output(self.name(), workspace, args)?;
        let capped = cap_string(&diff, max_output_bytes);

        Ok(ToolResult::json(json!({
            "diff": capped.content,
            "truncated": capped.truncated,
        })))
    }
}

pub struct GitLogTool;

#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }

    fn description(&self) -> &str {
        "Show recent git commits. `verbosity` controls per-commit detail: \
         \"detailed\" (default) returns `sha`, `author_name`, `author_email`, and \
         `subject` for each commit; \"concise\" returns oneline entries (`sha` + \
         `subject` only), dropping author fields for a cheaper history listing."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer" },
                "verbosity": verbosity_schema()
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let limit = optional_usize(&input, "limit", DEFAULT_LOG_LIMIT).min(MAX_LOG_LIMIT);
        let verbosity = parse_verbosity(self.name(), &input)?;
        let pretty = "--format=%H%x1f%an%x1f%ae%x1f%s";
        let limit_arg = format!("-n{limit}");
        let output = git_output(self.name(), workspace, &["log", &limit_arg, pretty])?;
        // Concise: oneline (sha + subject only). Detailed (default): the full
        // byte-identical historical shape with author fields.
        let concise = verbosity.is_concise();
        let commits = output
            .lines()
            .filter_map(|line| parse_log_line(line, concise))
            .collect::<Vec<_>>();

        Ok(ToolResult::json(json!({ "commits": commits })))
    }
}

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Create a guarded git commit for tracked workspace changes. By default \
         the commit is refused while any untracked files are present; set \
         `allow_untracked` to true to commit the staged/tracked changes anyway \
         (untracked files are left untouched and are not added)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": { "type": "string" },
                "allow_untracked": {
                    "type": "boolean",
                    "description": "Commit tracked changes even when untracked \
                                    files are present. Untracked files are not \
                                    added. Defaults to false (refuse if any \
                                    untracked files exist)."
                }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::WorkspaceWrite
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let message = required_string(self.name(), &input, "message")?;
        validate_commit_message(message)?;
        let allow_untracked = input
            .get("allow_untracked")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let status = git_output(self.name(), workspace, &["status", "--porcelain=v1"])?;
        let files = parse_status(&status);
        if !allow_untracked && files.iter().any(|file| file["index"] == "untracked") {
            return Err(tool_failed(
                self.name(),
                "refusing to commit while untracked files are present",
            ));
        }
        // Only the staged/tracked changes are committed. `git add -u` never
        // stages untracked files, so they remain untracked regardless of
        // `allow_untracked`.
        let tracked_changes = files
            .iter()
            .filter(|file| file["index"] != "untracked")
            .count();
        if tracked_changes == 0 {
            return Err(tool_failed(self.name(), "no changes to commit"));
        }

        git_output(self.name(), workspace, &["add", "-u"])?;
        let output = git_output(self.name(), workspace, &["commit", "-m", message])?;
        let head = git_output(self.name(), workspace, &["rev-parse", "HEAD"])?;
        let sha = head.trim();

        Ok(ToolResult::json(json!({
            "sha": sha,
            "subject": first_line(message),
            "files_changed": tracked_changes,
            "output": output.trim(),
        })))
    }
}

pub(crate) fn git_output(
    tool_name: &str,
    workspace: &Workspace,
    args: &[&str],
) -> Result<String, HarnessError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace.root())
        .output()
        .map_err(|error| tool_failed(tool_name, error.to_string()))?;

    if !output.status.success() {
        return Err(tool_failed(
            tool_name,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_status(status: &str) -> Vec<Value> {
    status
        .lines()
        .filter(|line| line.len() >= 3)
        .map(|line| {
            let index = &line[0..1];
            let worktree = &line[1..2];
            let path = line[3..].to_string();
            let (index, worktree) = if index == "?" && worktree == "?" {
                ("untracked", "untracked")
            } else {
                (status_label(index), status_label(worktree))
            };
            json!({
                "path": path,
                "index": index,
                "worktree": worktree,
            })
        })
        .collect()
}

fn status_label(value: &str) -> &'static str {
    match value {
        " " => "unmodified",
        "M" => "modified",
        "A" => "added",
        "D" => "deleted",
        "R" => "renamed",
        "C" => "copied",
        "U" => "unmerged",
        "?" => "untracked",
        _ => "other",
    }
}

fn parse_log_line(line: &str, concise: bool) -> Option<Value> {
    let mut parts = line.split('\x1f');
    let sha = parts.next()?;
    let author_name = parts.next()?;
    let author_email = parts.next()?;
    let subject = parts.next()?;

    if concise {
        return Some(json!({
            "sha": sha,
            "subject": subject,
        }));
    }

    Some(json!({
        "sha": sha,
        "author_name": author_name,
        "author_email": author_email,
        "subject": subject,
    }))
}

fn validate_commit_message(message: &str) -> Result<(), HarnessError> {
    if message.trim().is_empty() {
        return Err(HarnessError::InvalidToolInput {
            name: "git_commit".to_string(),
            message: "`message` must not be empty".to_string(),
        });
    }
    if message
        .lines()
        .any(|line| line.to_ascii_lowercase().starts_with("co-authored-by:"))
    {
        return Err(HarnessError::InvalidToolInput {
            name: "git_commit".to_string(),
            message: "coauthor trailers are not allowed".to_string(),
        });
    }

    Ok(())
}

pub(crate) struct CappedString {
    pub(crate) content: String,
    pub(crate) truncated: bool,
}

pub(crate) fn cap_string(value: &str, max_bytes: usize) -> CappedString {
    if value.len() <= max_bytes {
        return CappedString {
            content: value.to_string(),
            truncated: false,
        };
    }

    CappedString {
        content: value.chars().take(max_bytes).collect(),
        truncated: true,
    }
}

fn optional_usize(input: &Value, key: &str, default: usize) -> usize {
    input
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
}

fn tool_failed(name: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::ToolFailed {
        name: name.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    /// Initialize a git repo in a tempdir with one commit, then return a
    /// workspace over it. `modify` optionally rewrites `file.txt` afterward so
    /// `git diff` has something to report.
    fn repo_with_commit(modify: Option<&str>) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        fs::write(root.join("file.txt"), "line one\nline two\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial commit"]);
        if let Some(content) = modify {
            fs::write(root.join("file.txt"), content).unwrap();
        }
        let workspace = Workspace::new(root).unwrap();
        (dir, workspace)
    }

    #[tokio::test]
    async fn git_diff_default_equals_detailed_full_diff() {
        let (_dir, ws) = repo_with_commit(Some("line one\nCHANGED\n"));

        let default = GitDiffTool.execute(&ws, json!({})).await.unwrap();
        let detailed = GitDiffTool
            .execute(&ws, json!({ "verbosity": "detailed" }))
            .await
            .unwrap();

        assert_eq!(default.content(), detailed.content());
        let diff = default.content()["diff"].as_str().unwrap();
        // Full unified diff carries hunk headers and the changed lines.
        assert!(diff.contains("@@"), "expected unified diff, got: {diff}");
        assert!(diff.contains("CHANGED"));
    }

    #[tokio::test]
    async fn git_diff_concise_returns_stat_summary() {
        let (_dir, ws) = repo_with_commit(Some("line one\nCHANGED\n"));

        let result = GitDiffTool
            .execute(&ws, json!({ "verbosity": "concise" }))
            .await
            .unwrap();

        let diff = result.content()["diff"].as_str().unwrap();
        // `--stat` summary: file name + a changed-lines count, no hunk headers.
        assert!(diff.contains("file.txt"), "expected file in stat: {diff}");
        assert!(
            diff.contains("changed") || diff.contains("+"),
            "expected stat summary: {diff}"
        );
        assert!(
            !diff.contains("@@"),
            "stat must not include hunk headers: {diff}"
        );
    }

    #[tokio::test]
    async fn git_log_default_equals_detailed_with_author_fields() {
        let (_dir, ws) = repo_with_commit(None);

        let default = GitLogTool.execute(&ws, json!({})).await.unwrap();
        let detailed = GitLogTool
            .execute(&ws, json!({ "verbosity": "detailed" }))
            .await
            .unwrap();

        assert_eq!(default.content(), detailed.content());
        let commit = &default.content()["commits"][0];
        assert_eq!(commit["subject"], "initial commit");
        assert!(commit["author_name"].is_string());
        assert!(commit["author_email"].is_string());
        assert!(commit["sha"].is_string());
    }

    #[tokio::test]
    async fn git_commit_refuses_untracked_by_default() {
        let (_dir, ws) = repo_with_commit(Some("line one\nCHANGED\n"));
        fs::write(ws.root().join("new.txt"), "untracked\n").unwrap();

        let err = GitCommitTool
            .execute(&ws, json!({ "message": "edit" }))
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("untracked files are present"),
            "expected untracked refusal, got: {err}"
        );
    }

    #[tokio::test]
    async fn git_commit_allow_untracked_commits_tracked_and_leaves_untracked() {
        let (_dir, ws) = repo_with_commit(Some("line one\nCHANGED\n"));
        fs::write(ws.root().join("new.txt"), "untracked\n").unwrap();

        let result = GitCommitTool
            .execute(&ws, json!({ "message": "edit", "allow_untracked": true }))
            .await
            .expect("commit should succeed with allow_untracked");

        // Only the tracked file is counted/committed.
        assert_eq!(result.content()["files_changed"], 1);
        assert!(result.content()["sha"].is_string());

        // The untracked file is still untracked (not added).
        let status = git_output("git_status", &ws, &["status", "--porcelain=v1"]).unwrap();
        let files = parse_status(&status);
        assert!(
            files
                .iter()
                .any(|f| f["path"] == "new.txt" && f["index"] == "untracked"),
            "new.txt should remain untracked, status: {status}"
        );
        // The tracked change is gone (committed).
        assert!(
            !files.iter().any(|f| f["path"] == "file.txt"),
            "file.txt change should be committed, status: {status}"
        );
    }

    #[tokio::test]
    async fn git_commit_allow_untracked_with_only_untracked_has_nothing_to_commit() {
        let (_dir, ws) = repo_with_commit(None);
        fs::write(ws.root().join("new.txt"), "untracked\n").unwrap();

        let err = GitCommitTool
            .execute(&ws, json!({ "message": "edit", "allow_untracked": true }))
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("no changes to commit"),
            "expected no-changes error, got: {err}"
        );
    }

    #[tokio::test]
    async fn git_log_concise_is_oneline() {
        let (_dir, ws) = repo_with_commit(None);

        let result = GitLogTool
            .execute(&ws, json!({ "verbosity": "concise" }))
            .await
            .unwrap();

        let commit = &result.content()["commits"][0];
        assert_eq!(commit["subject"], "initial commit");
        assert!(commit["sha"].is_string());
        // Concise drops author fields.
        assert!(commit.get("author_name").is_none());
        assert!(commit.get("author_email").is_none());
    }
}
