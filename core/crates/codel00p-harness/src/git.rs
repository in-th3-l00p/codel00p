use std::process::Command;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

const DEFAULT_DIFF_BYTES: usize = 16_384;
const MAX_DIFF_BYTES: usize = 131_072;
const DEFAULT_LOG_LIMIT: usize = 10;
const MAX_LOG_LIMIT: usize = 100;

pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &'static str {
        "git_status"
    }

    fn description(&self) -> &'static str {
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
    fn name(&self) -> &'static str {
        "git_diff"
    }

    fn description(&self) -> &'static str {
        "Show git diff output for tracked workspace changes."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_output_bytes": { "type": "integer" }
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
        let diff = git_output(self.name(), workspace, &["diff", "--no-ext-diff"])?;
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
    fn name(&self) -> &'static str {
        "git_log"
    }

    fn description(&self) -> &'static str {
        "Show recent git commits."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer" }
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
        let pretty = "--format=%H%x1f%an%x1f%ae%x1f%s";
        let limit_arg = format!("-n{limit}");
        let output = git_output(self.name(), workspace, &["log", &limit_arg, pretty])?;
        let commits = output
            .lines()
            .filter_map(parse_log_line)
            .collect::<Vec<_>>();

        Ok(ToolResult::json(json!({ "commits": commits })))
    }
}

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &'static str {
        "git_commit"
    }

    fn description(&self) -> &'static str {
        "Create a guarded git commit for tracked workspace changes."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": { "type": "string" }
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

        let status = git_output(self.name(), workspace, &["status", "--porcelain=v1"])?;
        let files = parse_status(&status);
        if files.iter().any(|file| file["index"] == "untracked") {
            return Err(tool_failed(
                self.name(),
                "refusing to commit while untracked files are present",
            ));
        }
        if files.is_empty() {
            return Err(tool_failed(self.name(), "no changes to commit"));
        }

        git_output(self.name(), workspace, &["add", "-u"])?;
        let output = git_output(self.name(), workspace, &["commit", "-m", message])?;
        let head = git_output(self.name(), workspace, &["rev-parse", "HEAD"])?;
        let sha = head.trim();

        Ok(ToolResult::json(json!({
            "sha": sha,
            "subject": first_line(message),
            "files_changed": files.len(),
            "output": output.trim(),
        })))
    }
}

fn git_output(
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

fn parse_log_line(line: &str) -> Option<Value> {
    let mut parts = line.split('\x1f');
    let sha = parts.next()?;
    let author_name = parts.next()?;
    let author_email = parts.next()?;
    let subject = parts.next()?;

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

struct CappedString {
    content: String,
    truncated: bool,
}

fn cap_string(value: &str, max_bytes: usize) -> CappedString {
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
