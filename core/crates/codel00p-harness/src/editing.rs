use std::{fs, path::Path};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

pub struct CreateFileTool;

#[async_trait]
impl Tool for CreateFileTool {
    fn name(&self) -> &str {
        "create_file"
    }

    fn description(&self) -> &'static str {
        "Create a UTF-8 file inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
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
        let path = required_string(self.name(), &input, "path")?;
        let content = required_string(self.name(), &input, "content")?;
        let destination = workspace.resolve_for_create(path)?;
        if destination.exists() {
            return Err(tool_failed(
                self.name(),
                format!("file already exists: {path}"),
            ));
        }

        create_parent_dir(&destination)?;
        fs::write(&destination, content)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "created",
            "bytes_written": content.len(),
        })))
    }
}

pub struct UpdateFileTool;

#[async_trait]
impl Tool for UpdateFileTool {
    fn name(&self) -> &str {
        "update_file"
    }

    fn description(&self) -> &'static str {
        "Replace a UTF-8 file inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
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
        let path = required_string(self.name(), &input, "path")?;
        let content = required_string(self.name(), &input, "content")?;
        let destination = existing_file(workspace, self.name(), path)?;
        fs::write(&destination, content)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "updated",
            "bytes_written": content.len(),
        })))
    }
}

pub struct DeleteFileTool;

#[async_trait]
impl Tool for DeleteFileTool {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &'static str {
        "Delete a file inside the workspace root."
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

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::WorkspaceWrite
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let path = required_string(self.name(), &input, "path")?;
        let destination = existing_file(workspace, self.name(), path)?;
        fs::remove_file(destination)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "deleted",
        })))
    }
}

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &'static str {
        "Apply exact string replacements to workspace files."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["changes"],
            "properties": {
                "changes": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["path", "find", "replace"],
                        "properties": {
                            "path": { "type": "string" },
                            "find": { "type": "string" },
                            "replace": { "type": "string" }
                        }
                    }
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
        let changes = input
            .get("changes")
            .and_then(Value::as_array)
            .ok_or_else(|| HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "missing array field `changes`".to_string(),
            })?;
        if changes.is_empty() {
            return Err(HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "`changes` must not be empty".to_string(),
            });
        }

        let mut planned = Vec::new();
        for change in changes {
            let path = required_string(self.name(), change, "path")?;
            let find = required_string(self.name(), change, "find")?;
            let replace = required_string(self.name(), change, "replace")?;
            if find.is_empty() {
                return Err(HarnessError::InvalidToolInput {
                    name: self.name().to_string(),
                    message: "`find` must not be empty".to_string(),
                });
            }

            let destination = existing_file(workspace, self.name(), path)?;
            let original = fs::read_to_string(&destination)?;
            let replacements = original.matches(find).count();
            if replacements == 0 {
                return Err(tool_failed(
                    self.name(),
                    format!("find text was not present in {path}"),
                ));
            }
            let patched = original.replace(find, replace);
            planned.push((path.to_string(), destination, patched, replacements));
        }

        let mut summaries = Vec::new();
        for (path, destination, patched, replacements) in planned {
            fs::write(&destination, &patched)?;
            summaries.push(json!({
                "path": path,
                "replacements": replacements,
                "bytes_written": patched.len(),
            }));
        }

        Ok(ToolResult::json(json!({
            "operation": "patched",
            "changes": summaries,
        })))
    }
}

fn create_parent_dir(path: &Path) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn existing_file(
    workspace: &Workspace,
    tool_name: &str,
    path: &str,
) -> Result<std::path::PathBuf, HarnessError> {
    let destination = workspace.resolve_for_create(path)?;
    if !destination.is_file() {
        return Err(tool_failed(
            tool_name,
            format!("file does not exist: {path}"),
        ));
    }
    Ok(destination)
}

fn tool_failed(name: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::ToolFailed {
        name: name.to_string(),
        message: message.into(),
    }
}
