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

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use regex::RegexBuilder;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    walk::GlobMatcher,
    workspace::Workspace,
};

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

        let matcher = GlobMatcher::compile(self.name(), pattern)?;

        let mut files = Vec::new();
        workspace.walk(path, include_ignored, &mut |relative| {
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

        let mut files = Vec::new();
        workspace.walk(path, include_ignored, &mut |relative| {
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
            // Stat through the facade first and skip oversized files WITHOUT
            // reading them, so a pathological multi-GB file cannot be slurped
            // into memory. An unreadable/missing file is skipped (matching the
            // historical metadata/read failure handling).
            let Ok(meta) = workspace.metadata(relative) else {
                continue;
            };
            if meta.size > MAX_GREP_FILE_BYTES {
                continue;
            }
            // Read through the workspace facade so a remote backend reads remote
            // files. Unreadable or non-UTF-8 files are skipped (assumed
            // binary/generated).
            let Ok(bytes) = workspace.read_bytes(relative) else {
                continue;
            };
            let Ok(content) = String::from_utf8(bytes) else {
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

#[cfg(test)]
mod tests {
    use std::fs;

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
    async fn grep_skips_oversized_files_via_size_stat() {
        // An oversized file that also contains the needle must be skipped by the
        // metadata size-gate WITHOUT being read into memory. Only the small file
        // matches, proving the stat-before-read path works.
        let (dir, workspace) = workspace_with(&[("small.rs", "needle here\n")]);
        let mut big = vec![b'x'; (MAX_GREP_FILE_BYTES + 16) as usize];
        big[..7].copy_from_slice(b"needle\n");
        fs::write(dir.path().join("big.rs"), &big).unwrap();

        let result = GrepTool
            .execute(&workspace, json!({ "pattern": "needle" }))
            .await
            .unwrap();

        let matches = result.content()["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "small.rs");
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
