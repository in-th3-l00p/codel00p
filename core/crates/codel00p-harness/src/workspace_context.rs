//! Workspace / build-test awareness: a compact "Workspace state" system block
//! injected each turn so the agent knows the live state of the project without
//! rediscovering it every time (#12 T1.3).
//!
//! The block surfaces three best-effort facets, each omitted when there is
//! nothing to report so it stays cheap (it is paid for in tokens every turn):
//!
//! 1. **Git** — the current branch plus a short summary of the working-tree
//!    status (staged / modified / untracked counts). Gathered with a single
//!    `git status --porcelain=v1 --branch` shelled out at the workspace root.
//!    When the workspace is not a git repo (or git is absent / errors), the git
//!    section is simply omitted — gathering NEVER fails the turn.
//! 2. **Detected commands** — the project's test/build/lint commands from
//!    [`crate::checks::detect_checks`], so the model knows how to verify without
//!    guessing.
//! 3. **Recent edits** — files created/modified by the agent this turn, derived
//!    from the executed tool-call records.
//!
//! Plumbing mirrors the self block / base prompt: the harness assembles a
//! [`WorkspaceContext`], renders it, and attaches the rendered block to the
//! [`HarnessInferenceRequest`] via `with_workspace_context`; the provider adapter
//! emits it as a `ChatMessage::system(...)` block. It is situational state, so it
//! is placed after the instruction-ish blocks (self → base → instructions →
//! memory → skills → **workspace state**).

use std::process::Command;

use crate::{checks::DetectedChecks, turn::ExecutedToolCall, workspace::Workspace};

/// Maximum number of recently-edited file paths to list before collapsing the
/// rest into a "+N more" suffix, so the block stays compact.
const MAX_RECENT_EDITS: usize = 8;

/// The live workspace state gathered for one turn, ready to render into a compact
/// system block. Every facet is optional; an all-empty context renders to
/// `None` (no block).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceContext {
    /// Git working-tree summary, when the workspace is a git repo.
    git: Option<GitSummary>,
    /// The project's detected test/build/lint commands.
    detected: DetectedChecks,
    /// Workspace-relative paths the agent created/modified this turn (deduped,
    /// in first-seen order).
    recent_edits: Vec<String>,
}

/// A compact summary of `git status` for the workspace: the current branch and
/// counts of staged / modified / untracked entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitSummary {
    /// Current branch name, or `None` on a detached HEAD.
    pub branch: Option<String>,
    /// Entries with a staged (index) change.
    pub staged: usize,
    /// Tracked entries with an unstaged (worktree) change.
    pub modified: usize,
    /// Untracked entries.
    pub untracked: usize,
}

impl GitSummary {
    /// Whether the working tree is clean (no staged / modified / untracked).
    fn is_clean(&self) -> bool {
        self.staged == 0 && self.modified == 0 && self.untracked == 0
    }
}

impl WorkspaceContext {
    /// Gather the live workspace state: git status (best-effort), detected
    /// commands, and the files the agent touched this turn.
    ///
    /// `detected` is taken as a parameter (rather than re-detecting) so callers
    /// that already ran [`crate::checks::detect_checks`] can reuse it; the
    /// harness detects once per turn and passes it here.
    pub fn gather(
        workspace: &Workspace,
        detected: DetectedChecks,
        executed_tool_calls: &[ExecutedToolCall],
    ) -> Self {
        Self {
            git: gather_git_summary(workspace),
            detected,
            recent_edits: recent_edits(executed_tool_calls),
        }
    }

    /// The gathered git summary, if any.
    pub fn git(&self) -> Option<&GitSummary> {
        self.git.as_ref()
    }

    /// The detected commands.
    pub fn detected(&self) -> &DetectedChecks {
        &self.detected
    }

    /// The recently-edited paths.
    pub fn recent_edits(&self) -> &[String] {
        &self.recent_edits
    }

    /// Render the context into a compact system block, or `None` when there is
    /// nothing to report (every section empty).
    pub fn render(&self) -> Option<String> {
        let mut sections: Vec<String> = Vec::new();

        if let Some(git) = &self.git {
            sections.push(render_git(git));
        }

        if let Some(line) = render_detected(&self.detected) {
            sections.push(line);
        }

        if !self.recent_edits.is_empty() {
            sections.push(render_recent_edits(&self.recent_edits));
        }

        if sections.is_empty() {
            return None;
        }

        let mut lines = vec!["Workspace state (live; reflects the project right now):".to_string()];
        lines.extend(sections);
        Some(lines.join("\n"))
    }
}

/// Render the git section as a single line, e.g.
/// `- Git: branch `main`; 1 staged, 2 modified, 3 untracked.`
fn render_git(git: &GitSummary) -> String {
    let branch = match &git.branch {
        Some(branch) => format!("branch `{branch}`"),
        None => "detached HEAD".to_string(),
    };
    if git.is_clean() {
        return format!("- Git: {branch}; working tree clean.");
    }
    let mut parts: Vec<String> = Vec::new();
    if git.staged > 0 {
        parts.push(format!("{} staged", git.staged));
    }
    if git.modified > 0 {
        parts.push(format!("{} modified", git.modified));
    }
    if git.untracked > 0 {
        parts.push(format!("{} untracked", git.untracked));
    }
    format!("- Git: {branch}; {}.", parts.join(", "))
}

/// Render the detected-commands section, or `None` when nothing was detected.
fn render_detected(detected: &DetectedChecks) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(test) = &detected.test {
        parts.push(format!("test = `{test}`"));
    }
    if let Some(build) = &detected.build {
        parts.push(format!("build = `{build}`"));
    }
    if let Some(lint) = &detected.lint {
        parts.push(format!("lint = `{lint}`"));
    }
    if parts.is_empty() {
        return None;
    }
    let source = detected
        .source
        .as_deref()
        .map(|s| format!(" (from {s})"))
        .unwrap_or_default();
    Some(format!(
        "- Detected commands{source}: {}.",
        parts.join("; ")
    ))
}

/// Render the recently-edited-files section, capping the list at
/// [`MAX_RECENT_EDITS`] with a `+N more` suffix.
fn render_recent_edits(edits: &[String]) -> String {
    let shown: Vec<&str> = edits
        .iter()
        .take(MAX_RECENT_EDITS)
        .map(String::as_str)
        .collect();
    let mut line = format!("- Recent edits (this session): {}", shown.join(", "));
    if edits.len() > MAX_RECENT_EDITS {
        line.push_str(&format!(" (+{} more)", edits.len() - MAX_RECENT_EDITS));
    }
    line.push('.');
    line
}

/// Run `git status --porcelain=v1 --branch` at the workspace root and parse a
/// compact summary. Returns `None` when the workspace is not a git repo, git is
/// absent, or the command otherwise fails — gathering must never fail the turn.
fn gather_git_summary(workspace: &Workspace) -> Option<GitSummary> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--branch"])
        .current_dir(workspace.root())
        .output()
        .ok()?;
    if !output.status.success() {
        // Not a repo / git error — degrade gracefully (omit the section).
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(parse_status_branch(&text))
}

/// Parse the output of `git status --porcelain=v1 --branch` into a [`GitSummary`].
///
/// The first line is the branch header (`## <branch>...<upstream> [ahead N]`),
/// followed by one line per changed entry whose first two chars are the
/// index/worktree status codes (`??` = untracked).
fn parse_status_branch(text: &str) -> GitSummary {
    let mut summary = GitSummary::default();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            summary.branch = parse_branch_header(header);
            continue;
        }
        if line.len() < 2 {
            continue;
        }
        let bytes = line.as_bytes();
        let index = bytes[0] as char;
        let worktree = bytes[1] as char;
        if index == '?' && worktree == '?' {
            summary.untracked += 1;
            continue;
        }
        // Staged change: a non-space, non-`?` index code.
        if index != ' ' && index != '?' {
            summary.staged += 1;
        }
        // Unstaged (worktree) change: a non-space, non-`?` worktree code.
        if worktree != ' ' && worktree != '?' {
            summary.modified += 1;
        }
    }
    summary
}

/// Extract the local branch name from a porcelain `--branch` header line
/// (without the leading `## `). The header is `<branch>` or
/// `<branch>...<upstream> [ahead/behind ...]`; a detached HEAD reads
/// `HEAD (no branch)` which we treat as no branch name. An unborn branch (no
/// commits yet) reads `No commits yet on <branch>`, from which we recover the
/// branch name.
fn parse_branch_header(header: &str) -> Option<String> {
    // Unborn branch: `## No commits yet on main`.
    let header = header.strip_prefix("No commits yet on ").unwrap_or(header);
    // Strip the upstream-tracking suffix (`...origin/main [ahead 1]`).
    let local = header.split("...").next().unwrap_or(header).trim();
    // Some git versions append ` (no branch)` for a detached HEAD.
    if local.is_empty() || local == "HEAD (no branch)" || local.starts_with("HEAD (") {
        return None;
    }
    // Also handle a bare detached-HEAD header.
    if local == "HEAD" {
        return None;
    }
    Some(local.to_string())
}

/// Collect the workspace-relative paths the agent created/modified this turn from
/// the executed tool-call records, deduplicated in first-seen order.
///
/// Recognizes the mutating file tools by name (`create_file`, `edit_file`,
/// `write_file`, `delete_file`, `apply_patch`) and reads their `path` argument.
fn recent_edits(executed_tool_calls: &[ExecutedToolCall]) -> Vec<String> {
    let mut edits: Vec<String> = Vec::new();
    for call in executed_tool_calls {
        if !is_mutating_file_tool(&call.name) {
            continue;
        }
        // Skip calls that reported a failure — they did not change the file.
        if call.result.content().get("error").is_some() {
            continue;
        }
        if let Some(path) = call
            .input
            .get("path")
            .and_then(|value| value.as_str())
            .filter(|p| !p.is_empty())
        {
            let path = path.to_string();
            if !edits.contains(&path) {
                edits.push(path);
            }
        }
    }
    edits
}

/// Whether a tool name denotes a file-mutating tool whose `path` argument names
/// an edited file.
fn is_mutating_file_tool(name: &str) -> bool {
    matches!(
        name,
        "create_file" | "edit_file" | "write_file" | "delete_file" | "apply_patch"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_result::ToolResult;
    use serde_json::json;

    fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, content).unwrap();
        }
        let ws = Workspace::new(dir.path()).unwrap();
        (dir, ws)
    }

    fn edit_call(name: &str, path: &str) -> ExecutedToolCall {
        ExecutedToolCall {
            id: format!("call-{name}-{path}"),
            name: name.to_string(),
            input: json!({ "path": path }),
            result: ToolResult::json(json!({ "ok": true })),
        }
    }

    // --- parse_status_branch --------------------------------------------

    #[test]
    fn parses_branch_and_counts() {
        // Build line-by-line so leading-space status codes survive (a `\`-
        // continued string literal would strip the leading whitespace).
        let out = [
            "## main...origin/main [ahead 1]",
            "M  src/a.rs", // staged
            " M src/b.rs", // unstaged
            "MM src/c.rs", // both
            "?? new.txt",  // untracked
        ]
        .join("\n");
        let summary = parse_status_branch(&out);
        assert_eq!(summary.branch.as_deref(), Some("main"));
        // staged: `M ` (a.rs) + `MM` (c.rs) = 2
        assert_eq!(summary.staged, 2);
        // modified: ` M` (b.rs) + `MM` (c.rs) = 2
        assert_eq!(summary.modified, 2);
        assert_eq!(summary.untracked, 1);
    }

    #[test]
    fn parses_clean_tree() {
        let summary = parse_status_branch("## main\n");
        assert_eq!(summary.branch.as_deref(), Some("main"));
        assert!(summary.is_clean());
    }

    #[test]
    fn detached_head_has_no_branch() {
        let summary = parse_status_branch("## HEAD (no branch)\n M src/a.rs\n");
        assert_eq!(summary.branch, None);
        assert_eq!(summary.modified, 1);
    }

    #[test]
    fn branch_header_without_upstream() {
        assert_eq!(parse_branch_header("feat/x"), Some("feat/x".to_string()));
        assert_eq!(parse_branch_header("HEAD (no branch)"), None);
    }

    // --- recent_edits ----------------------------------------------------

    #[test]
    fn collects_dedup_mutating_paths_in_order() {
        let calls = vec![
            edit_call("create_file", "a.rs"),
            edit_call("edit_file", "b.rs"),
            edit_call("edit_file", "a.rs"), // dup -> dropped
            edit_call("read_file", "c.rs"), // non-mutating -> ignored
        ];
        assert_eq!(recent_edits(&calls), vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn skips_failed_edits() {
        let mut call = edit_call("edit_file", "broken.rs");
        call.result = ToolResult::json(json!({ "error": "no such file" }));
        assert!(recent_edits(&[call]).is_empty());
    }

    // --- render ----------------------------------------------------------

    #[test]
    fn renders_all_sections() {
        let ctx = WorkspaceContext {
            git: Some(GitSummary {
                branch: Some("feat/x".to_string()),
                staged: 1,
                modified: 2,
                untracked: 3,
            }),
            detected: DetectedChecks {
                test: Some("cargo test".to_string()),
                build: Some("cargo build".to_string()),
                lint: Some("cargo clippy".to_string()),
                source: Some("Cargo.toml".to_string()),
            },
            recent_edits: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
        };
        let block = ctx.render().expect("block present");
        assert!(block.contains("Workspace state"));
        assert!(block.contains("branch `feat/x`"));
        assert!(block.contains("1 staged, 2 modified, 3 untracked"));
        assert!(block.contains("test = `cargo test`"));
        assert!(block.contains("build = `cargo build`"));
        assert!(block.contains("lint = `cargo clippy`"));
        assert!(block.contains("(from Cargo.toml)"));
        assert!(block.contains("Recent edits"));
        assert!(block.contains("src/a.rs, src/b.rs"));
    }

    #[test]
    fn omits_empty_sections() {
        // Only detected commands present; git + edits omitted.
        let ctx = WorkspaceContext {
            git: None,
            detected: DetectedChecks {
                test: Some("pytest".to_string()),
                ..Default::default()
            },
            recent_edits: Vec::new(),
        };
        let block = ctx.render().unwrap();
        assert!(block.contains("test = `pytest`"));
        assert!(!block.contains("Git:"));
        assert!(!block.contains("Recent edits"));
    }

    #[test]
    fn renders_clean_git_tree() {
        let ctx = WorkspaceContext {
            git: Some(GitSummary {
                branch: Some("main".to_string()),
                ..Default::default()
            }),
            detected: DetectedChecks::default(),
            recent_edits: Vec::new(),
        };
        let block = ctx.render().unwrap();
        assert!(block.contains("working tree clean"));
    }

    #[test]
    fn fully_empty_context_renders_nothing() {
        let ctx = WorkspaceContext::default();
        assert!(ctx.render().is_none());
    }

    #[test]
    fn caps_recent_edits_with_more_suffix() {
        let edits: Vec<String> = (0..10).map(|i| format!("f{i}.rs")).collect();
        let line = render_recent_edits(&edits);
        assert!(line.contains("(+2 more)"));
        assert!(line.contains("f0.rs"));
        assert!(!line.contains("f8.rs"));
    }

    // --- gather (git integration) ---------------------------------------

    #[test]
    fn gather_omits_git_for_non_repo() {
        let (_dir, ws) = workspace_with(&[("README.md", "hi")]);
        let ctx = WorkspaceContext::gather(&ws, DetectedChecks::default(), &[]);
        // Not a git repo: git section omitted, never errors.
        assert!(ctx.git().is_none());
    }

    #[test]
    fn gather_reports_branch_for_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };
        if run(&["init", "-b", "trunk"]).status.success() {
            run(&["config", "user.email", "t@example.com"]);
            run(&["config", "user.name", "Test"]);
            std::fs::write(dir.path().join("a.txt"), "hi").unwrap();
            let ws = Workspace::new(dir.path()).unwrap();
            let detected = DetectedChecks {
                test: Some("cargo test".to_string()),
                ..Default::default()
            };
            let ctx = WorkspaceContext::gather(&ws, detected, &[]);
            let git = ctx.git().expect("git summary present for a real repo");
            assert_eq!(git.branch.as_deref(), Some("trunk"));
            assert_eq!(git.untracked, 1);
            let block = ctx.render().unwrap();
            assert!(block.contains("branch `trunk`"));
            assert!(block.contains("test = `cargo test`"));
        }
    }
}
