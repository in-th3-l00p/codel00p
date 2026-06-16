//! File-navigation tools for a coding agent: `find_files` (glob) and `grep`
//! (regular-expression content search).
//!
//! Both are read-only and concurrency-safe. They walk the workspace skipping
//! noisy build/VCS directories (`.git`, `target`, `node_modules`, …) by default
//! so the model is not flooded with vendored or generated files; pass
//! `include_ignored: true` to descend into them anyway.
//!
//! These complement the substring `search_text` and full-tree `list_files`
//! tools with the two capabilities a coding agent leans on most: finding files
//! by name/glob and searching contents by regex with surrounding context.

use std::{fs, path::Path};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    workspace::Workspace,
};

/// Directory names skipped by default while walking. These are the usual
/// build/dependency/VCS sinks that bloat results without adding signal.
const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".gradle",
    ".idea",
    ".vscode",
    "vendor",
];

/// Hard ceiling on files visited during a single walk, so a pathological tree
/// cannot hang a tool call.
const MAX_FILES_WALKED: usize = 100_000;
/// Default and ceiling for the number of `find_files` paths returned.
const DEFAULT_FIND_LIMIT: usize = 1_000;
const MAX_FIND_LIMIT: usize = 5_000;
/// Default and ceiling for the number of `grep` matches returned in one page.
const DEFAULT_GREP_LIMIT: usize = 200;
const MAX_GREP_LIMIT: usize = 1_000;
/// Files larger than this are skipped by `grep` (assumed binary/generated).
const MAX_GREP_FILE_BYTES: u64 = 5 * 1_024 * 1_024;
/// Upper bound on context lines emitted on each side of a match.
const MAX_CONTEXT_LINES: usize = 20;

/// Find files inside the workspace whose path matches a glob pattern.
pub struct FindFilesTool;

#[async_trait]
impl Tool for FindFilesTool {
    fn name(&self) -> &str {
        "find_files"
    }

    fn description(&self) -> &str {
        "Find files inside the workspace by glob pattern. `*` matches any run of \
         characters except `/`, `**` matches across directories, and `?` matches \
         a single character. A pattern with no `/` matches against the file name \
         anywhere in the tree (e.g. `*.rs` finds every Rust file); a pattern with \
         `/` matches against the path relative to `path` (e.g. `src/**/*.rs`). \
         Build and VCS directories are skipped unless `include_ignored` is true. \
         Results are sorted; use `limit` to cap them."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "include_ignored": { "type": "boolean" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_FIND_LIMIT }
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
        let pattern = required_string(self.name(), &input, "pattern")?;
        let path = optional_string(&input, "path").unwrap_or(".");
        let include_ignored = optional_bool(&input, "include_ignored");
        let limit = optional_usize(&input, "limit")
            .unwrap_or(DEFAULT_FIND_LIMIT)
            .clamp(1, MAX_FIND_LIMIT);

        let root = workspace.resolve(path)?;
        let matcher = GlobMatcher::compile(self.name(), pattern)?;

        let mut files = Vec::new();
        walk_files(workspace.root(), &root, include_ignored, &mut |relative| {
            files.push(relative.to_string())
        })?;
        files.sort();

        let mut matched: Vec<String> = files
            .into_iter()
            .filter(|relative| matcher.is_match(relative))
            .collect();
        let total = matched.len();
        let truncated = total > limit;
        matched.truncate(limit);

        Ok(ToolResult::json(json!({
            "pattern": pattern,
            "files": matched,
            "match_count": total,
            "truncated": truncated,
        })))
    }
}

/// Search file contents inside the workspace with a regular expression.
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents inside the workspace with a Rust regular expression \
         (regex crate syntax). Optionally restrict the files searched with `glob` \
         (same semantics as find_files), make the match case-insensitive, and emit \
         `context_lines` of surrounding text on each side of every hit. Build and \
         VCS directories are skipped unless `include_ignored` is true. Page through \
         hits with `offset` and `limit`."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "glob": { "type": "string" },
                "case_insensitive": { "type": "boolean" },
                "context_lines": { "type": "integer", "minimum": 0, "maximum": MAX_CONTEXT_LINES },
                "include_ignored": { "type": "boolean" },
                "offset": { "type": "integer", "minimum": 0 },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_GREP_LIMIT }
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
        let pattern = required_string(self.name(), &input, "pattern")?;
        let path = optional_string(&input, "path").unwrap_or(".");
        let case_insensitive = optional_bool(&input, "case_insensitive");
        let include_ignored = optional_bool(&input, "include_ignored");
        let context_lines = optional_usize(&input, "context_lines")
            .unwrap_or(0)
            .min(MAX_CONTEXT_LINES);
        let skip = optional_usize(&input, "offset").unwrap_or(0);
        let want = optional_usize(&input, "limit")
            .unwrap_or(DEFAULT_GREP_LIMIT)
            .clamp(1, MAX_GREP_LIMIT);

        let regex = RegexBuilder::new(pattern)
            .case_insensitive(case_insensitive)
            .build()
            .map_err(|error| HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: format!("invalid regex `{pattern}`: {error}"),
            })?;

        let glob = match optional_string(&input, "glob") {
            Some(glob) => Some(GlobMatcher::compile(self.name(), glob)?),
            None => None,
        };

        let root = workspace.resolve(path)?;
        let mut files = Vec::new();
        walk_files(workspace.root(), &root, include_ignored, &mut |relative| {
            if glob.as_ref().is_none_or(|g| g.is_match(relative)) {
                files.push(relative.to_string());
            }
        })?;
        files.sort();

        // Scan one past the requested window so we can report `has_more` without
        // reading every remaining match.
        let scan_cap = skip.saturating_add(want).saturating_add(1);
        let mut all = Vec::new();
        let mut files_with_matches = 0usize;
        'outer: for relative in &files {
            let absolute = workspace.resolve(relative)?;
            if fs::metadata(&absolute)
                .map(|meta| meta.len() > MAX_GREP_FILE_BYTES)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(content) = fs::read_to_string(&absolute) else {
                continue;
            };
            let lines: Vec<&str> = content.lines().collect();
            let mut file_matched = false;
            for (index, line) in lines.iter().enumerate() {
                if !regex.is_match(line) {
                    continue;
                }
                file_matched = true;
                all.push(build_match(relative, &lines, index, context_lines));
                if all.len() >= scan_cap {
                    if file_matched {
                        files_with_matches += 1;
                    }
                    break 'outer;
                }
            }
            if file_matched {
                files_with_matches += 1;
            }
        }

        let has_more = all.len() > skip.saturating_add(want);
        let window: Vec<Value> = all.into_iter().skip(skip).take(want).collect();

        Ok(ToolResult::json(json!({
            "pattern": pattern,
            "matches": window,
            "offset": skip,
            "has_more": has_more,
            "files_with_matches": files_with_matches,
        })))
    }
}

/// Build the JSON for a single match, including any requested context lines.
fn build_match(path: &str, lines: &[&str], index: usize, context: usize) -> Value {
    let mut value = json!({
        "path": path,
        "line": index + 1,
        "snippet": lines[index],
    });
    if context > 0 {
        let start = index.saturating_sub(context);
        let before: Vec<&str> = lines[start..index].to_vec();
        let after_end = (index + 1 + context).min(lines.len());
        let after: Vec<&str> = lines[index + 1..after_end].to_vec();
        value["before"] = json!(before);
        value["after"] = json!(after);
        value["before_start_line"] = json!(start + 1);
    }
    value
}

/// Read an optional boolean field, defaulting to `false`.
fn optional_bool(input: &Value, key: &str) -> bool {
    input.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Read an optional non-negative integer field as a `usize`.
fn optional_usize(input: &Value, key: &str) -> Option<usize> {
    input
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

/// Recursively visit every file under `current`, invoking `visit` with each
/// file's path relative to `root` (normalized with `/` separators). Skips the
/// default build/VCS directories unless `include_ignored` is set, and bails out
/// after [`MAX_FILES_WALKED`] files as a runaway guard.
fn walk_files(
    root: &Path,
    current: &Path,
    include_ignored: bool,
    visit: &mut dyn FnMut(&str),
) -> Result<(), HarnessError> {
    let mut visited = 0usize;
    walk_files_inner(root, current, include_ignored, &mut visited, visit)
}

fn walk_files_inner(
    root: &Path,
    current: &Path,
    include_ignored: bool,
    visited: &mut usize,
    visit: &mut dyn FnMut(&str),
) -> Result<(), HarnessError> {
    if current.is_file() {
        if let Ok(relative) = current.strip_prefix(root) {
            visit(&normalize_path(relative));
            *visited += 1;
        }
        return Ok(());
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if !include_ignored
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| DEFAULT_IGNORED_DIRS.contains(&name))
            {
                continue;
            }
            walk_files_inner(root, &path, include_ignored, visited, visit)?;
        } else if path.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            visit(&normalize_path(relative));
            *visited += 1;
            if *visited >= MAX_FILES_WALKED {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A compiled glob. Matches against the file name only when the source pattern
/// contains no `/`, otherwise against the full relative path.
struct GlobMatcher {
    regex: Regex,
    /// True when the pattern has no path separator and should match the file
    /// name rather than the full relative path.
    basename_only: bool,
}

impl GlobMatcher {
    fn compile(tool: &str, pattern: &str) -> Result<Self, HarnessError> {
        let basename_only = !pattern.contains('/');
        let regex = Regex::new(&glob_to_regex(pattern)).map_err(|error| {
            HarnessError::InvalidToolInput {
                name: tool.to_string(),
                message: format!("invalid glob `{pattern}`: {error}"),
            }
        })?;
        Ok(Self {
            regex,
            basename_only,
        })
    }

    fn is_match(&self, relative_path: &str) -> bool {
        let candidate = if self.basename_only {
            relative_path.rsplit('/').next().unwrap_or(relative_path)
        } else {
            relative_path
        };
        self.regex.is_match(candidate)
    }
}

/// Translate a glob pattern into an anchored regular expression.
///
/// `**` matches any run of characters (including `/`), `*` matches any run that
/// does not cross a `/`, and `?` matches a single non-`/` character. Every other
/// character is matched literally (regex metacharacters are escaped).
fn glob_to_regex(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() + 8);
    out.push('^');
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    out.push_str(".*");
                    i += 2;
                    // Swallow a `/` immediately after `**` so `**/foo` also
                    // matches `foo` at the root.
                    if i < chars.len() && chars[i] == '/' {
                        out.push_str("/?");
                        i += 1;
                    }
                    continue;
                }
                out.push_str("[^/]*");
            }
            '?' => out.push_str("[^/]"),
            c => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, content).unwrap();
        }
        let workspace = Workspace::new(dir.path()).unwrap();
        (dir, workspace)
    }

    #[test]
    fn glob_to_regex_handles_stars_and_question() {
        assert_eq!(glob_to_regex("*.rs"), "^[^/]*\\.rs$");
        assert_eq!(glob_to_regex("src/**/*.rs"), "^src/.*/?[^/]*\\.rs$");
        assert_eq!(glob_to_regex("a?c"), "^a[^/]c$");
    }

    #[test]
    fn glob_double_star_matches_root_and_nested() {
        let matcher = GlobMatcher::compile("find_files", "src/**/mod.rs").unwrap();
        assert!(matcher.is_match("src/mod.rs"));
        assert!(matcher.is_match("src/a/b/mod.rs"));
        assert!(!matcher.is_match("other/mod.rs"));
    }

    #[test]
    fn glob_basename_matches_anywhere() {
        let matcher = GlobMatcher::compile("find_files", "*.rs").unwrap();
        assert!(matcher.is_match("deep/nested/lib.rs"));
        assert!(!matcher.is_match("deep/nested/lib.py"));
    }

    #[tokio::test]
    async fn find_files_matches_glob_and_skips_ignored() {
        let (_dir, workspace) = workspace_with(&[
            ("src/main.rs", "fn main() {}"),
            ("src/util/helper.rs", "pub fn h() {}"),
            ("README.md", "# hi"),
            ("target/debug/build.rs", "generated"),
            ("node_modules/pkg/index.rs", "vendored"),
        ]);

        let result = FindFilesTool
            .execute(&workspace, json!({ "pattern": "*.rs" }))
            .await
            .unwrap();

        let files = result.content()["files"].as_array().unwrap();
        let names: Vec<&str> = files.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec!["src/main.rs", "src/util/helper.rs"]);
    }

    #[tokio::test]
    async fn find_files_can_include_ignored() {
        let (_dir, workspace) = workspace_with(&[
            ("src/main.rs", "fn main() {}"),
            ("target/debug/build.rs", "generated"),
        ]);

        let result = FindFilesTool
            .execute(
                &workspace,
                json!({ "pattern": "**/*.rs", "include_ignored": true }),
            )
            .await
            .unwrap();

        let files = result.content()["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn find_files_path_scoped_glob() {
        let (_dir, workspace) = workspace_with(&[
            ("src/a.rs", ""),
            ("src/nested/b.rs", ""),
            ("tests/c.rs", ""),
        ]);

        let result = FindFilesTool
            .execute(&workspace, json!({ "pattern": "src/**/*.rs" }))
            .await
            .unwrap();

        let files = result.content()["files"].as_array().unwrap();
        let names: Vec<&str> = files.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec!["src/a.rs", "src/nested/b.rs"]);
    }

    #[tokio::test]
    async fn grep_finds_regex_matches() {
        let (_dir, workspace) = workspace_with(&[
            ("a.rs", "fn alpha() {}\nfn beta() {}\n"),
            ("b.rs", "struct Gamma;\n"),
        ]);

        let result = GrepTool
            .execute(&workspace, json!({ "pattern": "fn \\w+" }))
            .await
            .unwrap();

        let matches = result.content()["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["path"], "a.rs");
        assert_eq!(matches[0]["line"], 1);
        assert_eq!(result.content()["files_with_matches"], 1);
    }

    #[tokio::test]
    async fn grep_case_insensitive_and_glob_filter() {
        let (_dir, workspace) =
            workspace_with(&[("a.rs", "TODO: rust\n"), ("b.txt", "todo: text\n")]);

        let result = GrepTool
            .execute(
                &workspace,
                json!({ "pattern": "todo", "case_insensitive": true, "glob": "*.rs" }),
            )
            .await
            .unwrap();

        let matches = result.content()["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "a.rs");
    }

    #[tokio::test]
    async fn grep_emits_context_lines() {
        let (_dir, workspace) = workspace_with(&[("a.rs", "one\ntwo\nTARGET\nfour\nfive\n")]);

        let result = GrepTool
            .execute(
                &workspace,
                json!({ "pattern": "TARGET", "context_lines": 1 }),
            )
            .await
            .unwrap();

        let m = &result.content()["matches"][0];
        assert_eq!(m["before"], json!(["two"]));
        assert_eq!(m["after"], json!(["four"]));
        assert_eq!(m["before_start_line"], 2);
    }

    #[tokio::test]
    async fn grep_paginates_with_offset_and_limit() {
        let (_dir, workspace) = workspace_with(&[("a.rs", "x\nx\nx\nx\nx\n")]);

        let result = GrepTool
            .execute(
                &workspace,
                json!({ "pattern": "x", "offset": 1, "limit": 2 }),
            )
            .await
            .unwrap();

        let content = result.content();
        let matches = content["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["line"], 2);
        assert_eq!(content["has_more"], true);
        assert_eq!(content["offset"], 1);
    }

    #[tokio::test]
    async fn grep_rejects_invalid_regex() {
        let (_dir, workspace) = workspace_with(&[("a.rs", "hi")]);

        let error = GrepTool
            .execute(&workspace, json!({ "pattern": "(" }))
            .await
            .unwrap_err();

        assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
    }
}
