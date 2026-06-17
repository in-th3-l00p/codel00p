use std::fs;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    errors::HarnessError, tool_result::ToolResult, walk::walk_files, workspace::Workspace,
};
use codel00p_protocol::PermissionScope;

const MAX_LISTED_FILES: usize = 1_000;
const MAX_SEARCH_MATCHES: usize = 200;

/// A tool's model-facing definition: the name, description, and JSON Schema the
/// model needs to call it. Produced from a [`Tool`] and sent to the provider, so
/// the model sees real parameters instead of a stub.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Builds a spec from a tool's trait methods.
    pub fn from_tool(tool: &dyn Tool) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn input_schema(&self) -> Value;

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        false
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::WorkspaceWrite
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError>;
}

pub struct ListFilesTool;

#[async_trait]
impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List files inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
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
        let path = optional_string(&input, "path").unwrap_or(".");
        let root = workspace.resolve(path)?;
        let mut files = Vec::new();

        // Ignore-aware and resilient: skips build/VCS sinks (`.git`, `target`,
        // `node_modules`, …) and tolerates unreadable nested directories instead
        // of aborting the whole listing.
        walk_files(workspace.root(), &root, false, &mut |relative| {
            files.push(relative.to_string());
        })?;
        files.sort();
        files.truncate(MAX_LISTED_FILES);

        Ok(ToolResult::json(json!({ "files": files })))
    }
}

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a UTF-8 file inside the workspace root. Optionally read a line window \
         with `offset` (1-based start line) and `limit` (max lines) to avoid \
         loading a huge file into context."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer" }
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
        let path = required_string(self.name(), &input, "path")?;
        let content = workspace.read_utf8(path)?;

        let offset = optional_u64(&input, "offset");
        let limit = optional_u64(&input, "limit");

        // Without a window, return the whole file (unchanged behavior).
        if offset.is_none() && limit.is_none() {
            return Ok(ToolResult::json(json!({
                "path": path,
                "content": content,
            })));
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let start = (offset.unwrap_or(1).max(1) as usize)
            .saturating_sub(1)
            .min(total_lines);
        let count = limit.map(|l| l as usize).unwrap_or(total_lines - start);
        let end = start.saturating_add(count).min(total_lines);
        let window = lines[start..end].join("\n");

        Ok(ToolResult::json(json!({
            "path": path,
            "content": window,
            "start_line": start + 1,
            "line_count": end - start,
            "total_lines": total_lines,
            "has_more": end < total_lines,
        })))
    }
}

pub struct SearchTextTool;

#[async_trait]
impl Tool for SearchTextTool {
    fn name(&self) -> &str {
        "search_text"
    }

    fn description(&self) -> &str {
        "Search UTF-8 files inside the workspace root. Page through results with \
         `offset` (matches to skip) and `limit` (max matches to return)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer" }
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
        let query = required_string(self.name(), &input, "query")?;
        let path = optional_string(&input, "path").unwrap_or(".");
        let offset = optional_u64(&input, "offset");
        let limit = optional_u64(&input, "limit");
        let root = workspace.resolve(path)?;
        let mut files = Vec::new();
        // Ignore-aware and resilient: skips build/VCS sinks (`.git`, `target`,
        // `node_modules`, …) and tolerates unreadable nested directories instead
        // of aborting the whole search.
        walk_files(workspace.root(), &root, false, &mut |relative| {
            files.push(relative.to_string());
        })?;
        files.sort();

        // The window of matches the caller wants, plus one extra to detect
        // `has_more` without scanning everything. A bare search keeps the legacy
        // cap and output shape.
        let skip = offset.unwrap_or(0) as usize;
        let want = limit.map(|l| l as usize).unwrap_or(MAX_SEARCH_MATCHES);
        let scan_cap = skip
            .saturating_add(want)
            .saturating_add(1)
            .min(MAX_SEARCH_MATCHES);

        let mut all = Vec::new();
        'outer: for relative in files {
            let absolute = workspace.resolve(&relative)?;
            let Ok(content) = fs::read_to_string(absolute) else {
                continue;
            };
            for (index, line) in content.lines().enumerate() {
                if line.contains(query) {
                    all.push(json!({
                        "path": relative,
                        "line": index + 1,
                        "snippet": line,
                    }));
                    if all.len() >= scan_cap {
                        break 'outer;
                    }
                }
            }
        }

        if offset.is_none() && limit.is_none() {
            return Ok(ToolResult::json(json!({ "matches": all })));
        }

        let has_more = all.len() > skip + want;
        let window: Vec<Value> = all.into_iter().skip(skip).take(want).collect();
        Ok(ToolResult::json(json!({
            "matches": window,
            "offset": skip,
            "has_more": has_more,
        })))
    }
}

/// Reads an optional non-negative integer field from tool input.
fn optional_u64(input: &Value, field: &str) -> Option<u64> {
    input.get(field).and_then(Value::as_u64)
}

pub(crate) fn required_string<'a>(
    tool: &str,
    input: &'a Value,
    key: &str,
) -> Result<&'a str, HarnessError> {
    optional_string(input, key).ok_or_else(|| HarnessError::InvalidToolInput {
        name: tool.to_string(),
        message: format!("missing string field `{key}`"),
    })
}

pub(crate) fn optional_string<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
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

    fn file_names(result: &ToolResult) -> Vec<String> {
        result.content()["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn list_files_skips_ignored_dirs() {
        let (_dir, workspace) = workspace_with(&[
            ("src/main.rs", "fn main() {}"),
            ("README.md", "# hi"),
            ("target/debug/build.rs", "generated"),
            (".git/config", "[core]"),
            ("node_modules/pkg/index.js", "module.exports = {}"),
        ]);

        let result = ListFilesTool.execute(&workspace, json!({})).await.unwrap();

        let names = file_names(&result);
        assert_eq!(names, vec!["README.md", "src/main.rs"]);
        assert!(!names.iter().any(|n| n.starts_with("target/")));
        assert!(!names.iter().any(|n| n.starts_with(".git/")));
        assert!(!names.iter().any(|n| n.starts_with("node_modules/")));
    }

    #[tokio::test]
    async fn search_text_skips_ignored_dirs() {
        let (_dir, workspace) = workspace_with(&[
            ("src/lib.rs", "let needle = 1;"),
            ("target/debug/gen.rs", "let needle = 2;"),
            ("node_modules/pkg/x.js", "var needle = 3;"),
        ]);

        let result = SearchTextTool
            .execute(&workspace, json!({ "query": "needle" }))
            .await
            .unwrap();

        let matches = result.content()["matches"].as_array().unwrap();
        let paths: Vec<&str> = matches
            .iter()
            .map(|m| m["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["src/lib.rs"]);
    }

    /// Regression test for the macOS-TCC / EPERM bug: a recursive walk must not
    /// abort the whole listing when it hits an unreadable subdirectory. We
    /// create a readable file alongside a `0o000` directory and assert
    /// `list_files` succeeds and returns the readable file.
    #[cfg(unix)]
    #[tokio::test]
    async fn list_files_tolerates_unreadable_subdir() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, workspace) =
            workspace_with(&[("readable.txt", "hello"), ("locked/secret.txt", "nope")]);
        let locked = dir.path().join("locked");

        // Restore permissions on drop so the tempdir can be cleaned up even if
        // an assertion below panics.
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let _restore = Restore(locked.clone());

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let result = ListFilesTool
            .execute(&workspace, json!({}))
            .await
            .expect("list_files must not abort on an unreadable subdir");

        let names = file_names(&result);
        assert!(
            names.iter().any(|n| n == "readable.txt"),
            "expected readable file in results, got {names:?}"
        );
    }

    /// The same resilience must hold for `search_text`, which gathers candidate
    /// files via the shared walk before grepping them.
    #[cfg(unix)]
    #[tokio::test]
    async fn search_text_tolerates_unreadable_subdir() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, workspace) = workspace_with(&[
            ("readable.txt", "needle here"),
            ("locked/secret.txt", "needle hidden"),
        ]);
        let locked = dir.path().join("locked");

        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let _restore = Restore(locked.clone());

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let result = SearchTextTool
            .execute(&workspace, json!({ "query": "needle" }))
            .await
            .expect("search_text must not abort on an unreadable subdir");

        let matches = result.content()["matches"].as_array().unwrap();
        let paths: Vec<&str> = matches
            .iter()
            .map(|m| m["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["readable.txt"]);
    }
}
