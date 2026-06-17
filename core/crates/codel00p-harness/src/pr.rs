use std::process::Command;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, tools::Tool, workspace::Workspace};

const DEFAULT_DIFF_BYTES: usize = 16_384;
const MAX_DIFF_BYTES: usize = 131_072;

pub struct PreparePrTool;

#[async_trait]
impl Tool for PreparePrTool {
    fn name(&self) -> &str {
        "prepare_pr"
    }

    fn description(&self) -> &str {
        "Summarize the current branch against a base branch into PR-ready material \
         (commits, changed files, diffstat, draft body). Read-only; does not push."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "base": {
                    "type": "string",
                    "description": "Base branch to compare against. Defaults to `main`, falling back to `master`."
                },
                "head": {
                    "type": "string",
                    "description": "Head ref to compare. Defaults to `HEAD`."
                },
                "max_diff_bytes": {
                    "type": "integer",
                    "description": "Cap on diff preview size in bytes. Defaults to 16 KiB."
                }
            }
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let head = input
            .get("head")
            .and_then(Value::as_str)
            .unwrap_or("HEAD")
            .to_string();

        let max_diff_bytes = input
            .get("max_diff_bytes")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_DIFF_BYTES)
            .min(MAX_DIFF_BYTES);

        // Resolve base: use provided value, else try "main", else "master"
        let base = if let Some(b) = input.get("base").and_then(Value::as_str) {
            b.to_string()
        } else {
            resolve_default_base(self.name(), workspace)?
        };

        // Resolve head ref to a concrete SHA
        let head_sha = git_output(self.name(), workspace, &["rev-parse", &head])?;
        let head_sha = head_sha.trim().to_string();

        // Collect commits: base..head
        let pretty = "--format=%H%x1f%an%x1f%ae%x1f%s";
        let range = format!("{base}..{head}");
        let log_out = git_output(self.name(), workspace, &["log", &range, pretty])?;
        let commits: Vec<Value> = log_out
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(parse_log_line)
            .collect();

        // Collect numstat: base...head (three-dot for symmetric diff from merge-base)
        let three_dot = format!("{base}...{head}");
        let numstat_out = git_output(self.name(), workspace, &["diff", "--numstat", &three_dot])?;
        let files_changed: Vec<Value> = numstat_out
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(parse_numstat_line)
            .collect();

        // Compute diffstat totals
        let total_files = files_changed.len();
        let total_additions: i64 = files_changed
            .iter()
            .map(|f| f["additions"].as_i64().unwrap_or(0))
            .sum();
        let total_deletions: i64 = files_changed
            .iter()
            .map(|f| f["deletions"].as_i64().unwrap_or(0))
            .sum();

        // Diff preview
        let diff_raw = git_output(self.name(), workspace, &["diff", &three_dot])?;
        let capped = cap_string(&diff_raw, max_diff_bytes);

        // Determine pr_ready
        let pr_ready = !commits.is_empty() && !files_changed.is_empty();

        // Build draft body
        let draft_body = build_draft_body(&commits, &files_changed);

        // Title suggestion
        let title_suggestion = if commits.len() == 1 {
            commits[0]["subject"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        } else {
            // Derive from branch name
            let branch_out = git_output(
                self.name(),
                workspace,
                &["rev-parse", "--abbrev-ref", "HEAD"],
            )
            .unwrap_or_default();
            let branch = branch_out.trim();
            // Convert branch name to a readable title: strip common prefixes, replace
            // hyphens/underscores/slashes with spaces
            branch_to_title(branch)
        };

        Ok(ToolResult::json(json!({
            "base": base,
            "head": head_sha,
            "pr_ready": pr_ready,
            "commits": commits,
            "files_changed": files_changed,
            "diffstat": {
                "files": total_files,
                "additions": total_additions,
                "deletions": total_deletions,
            },
            "diff_preview": capped.content,
            "truncated": capped.truncated,
            "draft_body": draft_body,
            "title_suggestion": title_suggestion,
        })))
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn resolve_default_base(tool_name: &str, workspace: &Workspace) -> Result<String, HarnessError> {
    // Try "main" first
    if git_output(tool_name, workspace, &["rev-parse", "--verify", "main"]).is_ok() {
        return Ok("main".to_string());
    }
    // Fallback to "master"
    if git_output(tool_name, workspace, &["rev-parse", "--verify", "master"]).is_ok() {
        return Ok("master".to_string());
    }
    Err(tool_failed(
        tool_name,
        "could not detect a base branch; specify `base` explicitly (tried `main`, `master`)",
    ))
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
        .map_err(|e| tool_failed(tool_name, e.to_string()))?;

    if !output.status.success() {
        return Err(tool_failed(
            tool_name,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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

/// Parse a single `git diff --numstat` line.
///
/// Format: `<additions>\t<deletions>\t<path>` — binary files show `-` for both counts.
fn parse_numstat_line(line: &str) -> Option<Value> {
    let mut parts = line.splitn(3, '\t');
    let additions_raw = parts.next()?;
    let deletions_raw = parts.next()?;
    let path = parts.next()?.to_string();

    // Binary files use `-`; treat them as 0 for summation but preserve the
    // fact they changed via the path entry.
    let additions: i64 = additions_raw.parse().unwrap_or(0);
    let deletions: i64 = deletions_raw.parse().unwrap_or(0);

    Some(json!({
        "path": path,
        "additions": additions,
        "deletions": deletions,
    }))
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

fn build_draft_body(commits: &[Value], files: &[Value]) -> String {
    let mut body = String::new();

    body.push_str("## Summary\n\n");
    if commits.is_empty() {
        body.push_str("_No commits ahead of base._\n");
    } else {
        for commit in commits {
            let subject = commit["subject"].as_str().unwrap_or("");
            body.push_str(&format!("- {subject}\n"));
        }
    }

    body.push('\n');
    body.push_str("## Files changed\n\n");
    if files.is_empty() {
        body.push_str("_No files changed._\n");
    } else {
        for file in files {
            let path = file["path"].as_str().unwrap_or("");
            let additions = file["additions"].as_i64().unwrap_or(0);
            let deletions = file["deletions"].as_i64().unwrap_or(0);
            body.push_str(&format!("- `{path}` (+{additions} / -{deletions})\n"));
        }
    }

    body
}

/// Convert a branch name to a human-readable title suggestion.
///
/// Strips common prefixes like `feature/`, `feat/`, `fix/`, `chore/` and
/// replaces separators with spaces.
fn branch_to_title(branch: &str) -> String {
    let stripped = branch
        .trim_start_matches("feature/")
        .trim_start_matches("feat/")
        .trim_start_matches("fix/")
        .trim_start_matches("chore/")
        .trim_start_matches("refactor/")
        .trim_start_matches("docs/");
    stripped.replace(['/', '-', '_'], " ")
}

fn tool_failed(name: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::ToolFailed {
        name: name.to_string(),
        message: message.into(),
    }
}
