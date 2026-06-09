use std::{fs, path::Path};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, workspace::Workspace};
use codel00p_protocol::PermissionScope;

const MAX_LISTED_FILES: usize = 1_000;
const MAX_SEARCH_MATCHES: usize = 200;

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
        "Read a UTF-8 file inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
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
        let path = required_string(self.name(), &input, "path")?;
        let content = workspace.read_utf8(path)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "content": content,
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
        "Search UTF-8 files inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string" },
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
        let query = required_string(self.name(), &input, "query")?;
        let path = optional_string(&input, "path").unwrap_or(".");
        let root = workspace.resolve(path)?;
        let mut files = Vec::new();
        collect_files(workspace.root(), &root, &mut files)?;
        files.sort();

        let mut matches = Vec::new();
        for relative in files {
            if matches.len() >= MAX_SEARCH_MATCHES {
                break;
            }

            let absolute = workspace.resolve(&relative)?;
            let Ok(content) = fs::read_to_string(absolute) else {
                continue;
            };

            for (index, line) in content.lines().enumerate() {
                if line.contains(query) {
                    matches.push(json!({
                        "path": relative,
                        "line": index + 1,
                        "snippet": line,
                    }));

                    if matches.len() >= MAX_SEARCH_MATCHES {
                        break;
                    }
                }
            }
        }

        Ok(ToolResult::json(json!({ "matches": matches })))
    }
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
