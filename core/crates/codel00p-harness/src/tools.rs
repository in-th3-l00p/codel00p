use std::{fs, path::Path};

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

        collect_files(workspace.root(), &root, &mut files)?;
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
        collect_files(workspace.root(), &root, &mut files)?;
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

fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) -> Result<(), HarnessError> {
    if current.is_file() {
        push_relative_file(root, current, files);
        return Ok(());
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            push_relative_file(root, &path, files);
        }
    }

    Ok(())
}

fn push_relative_file(root: &Path, path: &Path, files: &mut Vec<String>) {
    if let Ok(relative) = path.strip_prefix(root) {
        files.push(normalize_path(relative));
    }
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
