use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, workspace::Workspace};
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
        "List files inside the workspace root. Page through results with `offset` \
         (files to skip) and `limit` (max files to return). `verbosity` controls \
         the result shape: \"detailed\" (default) returns the bare relative paths \
         in a `files` array; \"concise\" returns the same paths but omits the \
         pagination envelope (`offset`/`has_more`/`total`) when paging — useful \
         when you only need the names."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "offset": { "type": "integer", "minimum": 0 },
                "limit": { "type": "integer", "minimum": 1 },
                "verbosity": verbosity_schema()
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
        let verbosity = parse_verbosity(self.name(), &input)?;
        let offset = optional_u64(&input, "offset");
        let limit = optional_u64(&input, "limit");
        let mut files = Vec::new();

        // Ignore-aware and resilient: skips build/VCS sinks (`.git`, `target`,
        // `node_modules`, …) and tolerates unreadable nested directories instead
        // of aborting the whole listing. Routed through the workspace facade so
        // the configured execution backend performs the traversal.
        workspace.walk(path, false, &mut |relative| {
            files.push(relative.to_string());
        })?;
        files.sort();

        // A bare call (no paging) keeps the legacy `MAX_LISTED_FILES` truncation
        // and the exact `{ "files": [...] }` shape — no behavior change.
        if offset.is_none() && limit.is_none() {
            files.truncate(MAX_LISTED_FILES);
            return Ok(ToolResult::json(json!({ "files": files })));
        }

        // Real pagination: skip `offset`, take `limit` (still bounded so a single
        // page cannot exceed the legacy cap), and report `has_more`/`total`.
        let total = files.len();
        let skip = offset.unwrap_or(0) as usize;
        let want = (limit.map(|l| l as usize).unwrap_or(MAX_LISTED_FILES)).min(MAX_LISTED_FILES);
        let page: Vec<String> = files.into_iter().skip(skip).take(want).collect();
        let has_more = skip.saturating_add(page.len()) < total;

        // Concise drops the pagination envelope; detailed includes it.
        if verbosity.is_concise() {
            return Ok(ToolResult::json(json!({ "files": page })));
        }
        Ok(ToolResult::json(json!({
            "files": page,
            "offset": skip,
            "has_more": has_more,
            "total": total,
        })))
    }
}

// NOTE: `read_file` intentionally has no `verbosity` param. Its result is the
// exact file content the caller asked for; there is no "summary" of a file that
// is both cheaper and still that file. Callers who want fewer tokens use the
// existing `offset`/`limit` line window instead.
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
                "offset": { "type": "integer", "minimum": 1 },
                "limit": { "type": "integer", "minimum": 1 }
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

        // Read raw bytes and decode gracefully: a non-UTF-8 file must NOT abort
        // the tool (previously `read_utf8` hard-errored with "invalid utf-8
        // sequence …"). A binary file (NUL byte present) is reported as such
        // rather than dumping garbage; other invalid-UTF-8 text (e.g. latin-1)
        // is lossily decoded so the model still sees the content, with a flag.
        let bytes = workspace.read_bytes(path)?;
        let (content, non_utf8) = match String::from_utf8(bytes) {
            Ok(text) => (text, false),
            Err(error) => {
                let raw = error.into_bytes();
                if raw.contains(&0) {
                    return Ok(ToolResult::json(json!({
                        "path": path,
                        "binary": true,
                        "byte_size": raw.len(),
                        "note": "File is not valid UTF-8 and appears to be binary; \
                                 not shown as text. Use a command tool if you need \
                                 to inspect its bytes.",
                    })));
                }
                (String::from_utf8_lossy(&raw).into_owned(), true)
            }
        };

        let offset = optional_u64(&input, "offset");
        let limit = optional_u64(&input, "limit");

        // Without a window, return the whole file.
        if offset.is_none() && limit.is_none() {
            let mut result = json!({
                "path": path,
                "content": content,
            });
            if non_utf8 {
                result["non_utf8"] = json!(true);
                result["note"] =
                    json!("File was not valid UTF-8; decoded lossily (invalid bytes replaced).");
            }
            return Ok(ToolResult::json(result));
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let start = (offset.unwrap_or(1).max(1) as usize)
            .saturating_sub(1)
            .min(total_lines);
        let count = limit.map(|l| l as usize).unwrap_or(total_lines - start);
        let end = start.saturating_add(count).min(total_lines);
        let window = lines[start..end].join("\n");

        let mut result = json!({
            "path": path,
            "content": window,
            "start_line": start + 1,
            "line_count": end - start,
            "total_lines": total_lines,
            "has_more": end < total_lines,
        });
        if non_utf8 {
            result["non_utf8"] = json!(true);
        }
        Ok(ToolResult::json(result))
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
        let mut files = Vec::new();
        // Ignore-aware and resilient: skips build/VCS sinks (`.git`, `target`,
        // `node_modules`, …) and tolerates unreadable nested directories instead
        // of aborting the whole search. Routed through the workspace facade.
        workspace.walk(path, false, &mut |relative| {
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
            let Ok(content) = workspace.read_utf8(&relative) else {
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

/// Token-efficiency control shared by the heavy read/output tools.
///
/// `Detailed` is the default everywhere and is byte-identical to each tool's
/// historical output. `Concise` trims a tool's result to a cheaper shape (the
/// exact trimming is tool-specific and documented at each call site) so the
/// model can ask for a summary instead of always paying for full output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verbosity {
    Concise,
    Detailed,
}

impl Verbosity {
    pub(crate) fn is_concise(self) -> bool {
        matches!(self, Verbosity::Concise)
    }
}

/// Parse the shared `verbosity` field. Accepts `"concise"` or `"detailed"`;
/// an omitted field defaults to `Detailed` (no behavior change). Any other
/// value is rejected so typos surface instead of silently falling back.
pub(crate) fn parse_verbosity(tool: &str, input: &Value) -> Result<Verbosity, HarnessError> {
    match optional_string(input, "verbosity") {
        None => Ok(Verbosity::Detailed),
        Some("detailed") => Ok(Verbosity::Detailed),
        Some("concise") => Ok(Verbosity::Concise),
        Some(other) => Err(HarnessError::InvalidToolInput {
            name: tool.to_string(),
            message: format!("`verbosity` must be \"concise\" or \"detailed\", got `{other}`"),
        }),
    }
}

/// The JSON Schema fragment for the shared `verbosity` field, so every tool that
/// supports it advertises the same enum and default.
pub(crate) fn verbosity_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["concise", "detailed"],
        "default": "detailed"
    })
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

    fn file_names(result: &ToolResult) -> Vec<String> {
        result.content()["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    fn workspace_with_bytes(path: &str, bytes: &[u8]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(path), bytes).unwrap();
        let workspace = Workspace::new(dir.path()).unwrap();
        (dir, workspace)
    }

    #[test]
    fn read_file_schema_declares_offset_and_limit_minimums() {
        let schema = ReadFileTool.input_schema();
        let props = &schema["properties"];
        assert_eq!(props["offset"]["minimum"], json!(1));
        assert_eq!(props["limit"]["minimum"], json!(1));
    }

    #[tokio::test]
    async fn read_file_reports_binary_non_utf8_gracefully() {
        // A file with a NUL byte (classic binary signal) must not abort the tool
        // with "invalid utf-8 sequence …"; it is reported as binary.
        let (_dir, workspace) = workspace_with_bytes("blob.bin", &[0x00, 0xff, 0x10, 0x80]);
        let result = ReadFileTool
            .execute(&workspace, json!({ "path": "blob.bin" }))
            .await
            .expect("read_file must not error on a binary file");
        assert_eq!(result.content()["binary"], json!(true));
        assert_eq!(result.content()["byte_size"], json!(4));
        assert!(result.content().get("content").is_none());
    }

    #[tokio::test]
    async fn read_file_lossily_decodes_non_utf8_text() {
        // Latin-1 "café" (0xe9 for é) is invalid UTF-8 but text (no NUL). It is
        // decoded lossily so the model still sees content, flagged non_utf8.
        let (_dir, workspace) = workspace_with_bytes("notes.txt", b"caf\xe9\n");
        let result = ReadFileTool
            .execute(&workspace, json!({ "path": "notes.txt" }))
            .await
            .expect("read_file must not error on lossy-decodable text");
        assert_eq!(result.content()["non_utf8"], json!(true));
        let content = result.content()["content"].as_str().unwrap();
        assert!(content.starts_with("caf"), "got: {content:?}");
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
    async fn list_files_default_omits_pagination_envelope() {
        // No paging, no verbosity: byte-identical to the historical shape —
        // a bare `{ "files": [...] }` with no offset/has_more/total fields.
        let (_dir, workspace) = workspace_with(&[("a.txt", ""), ("b.txt", ""), ("c.txt", "")]);

        let result = ListFilesTool.execute(&workspace, json!({})).await.unwrap();

        let content = result.content();
        assert_eq!(file_names(&result), vec!["a.txt", "b.txt", "c.txt"]);
        assert!(content.get("offset").is_none());
        assert!(content.get("has_more").is_none());
        assert!(content.get("total").is_none());
    }

    #[tokio::test]
    async fn list_files_paginates_with_offset_and_limit() {
        let (_dir, workspace) =
            workspace_with(&[("a.txt", ""), ("b.txt", ""), ("c.txt", ""), ("d.txt", "")]);

        let result = ListFilesTool
            .execute(&workspace, json!({ "offset": 1, "limit": 2 }))
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(file_names(&result), vec!["b.txt", "c.txt"]);
        assert_eq!(content["offset"], 1);
        assert_eq!(content["has_more"], true);
        assert_eq!(content["total"], 4);
    }

    #[tokio::test]
    async fn list_files_concise_drops_pagination_envelope() {
        let (_dir, workspace) = workspace_with(&[("a.txt", ""), ("b.txt", ""), ("c.txt", "")]);

        let result = ListFilesTool
            .execute(
                &workspace,
                json!({ "offset": 0, "limit": 2, "verbosity": "concise" }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(file_names(&result), vec!["a.txt", "b.txt"]);
        // Concise pages without the envelope fields.
        assert!(content.get("offset").is_none());
        assert!(content.get("has_more").is_none());
        assert!(content.get("total").is_none());
    }

    #[tokio::test]
    async fn parse_verbosity_defaults_and_rejects_garbage() {
        assert_eq!(
            parse_verbosity("t", &json!({})).unwrap(),
            Verbosity::Detailed
        );
        assert_eq!(
            parse_verbosity("t", &json!({ "verbosity": "concise" })).unwrap(),
            Verbosity::Concise
        );
        assert_eq!(
            parse_verbosity("t", &json!({ "verbosity": "detailed" })).unwrap(),
            Verbosity::Detailed
        );
        let err = parse_verbosity("t", &json!({ "verbosity": "loud" })).unwrap_err();
        assert!(matches!(err, HarnessError::InvalidToolInput { .. }));
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
