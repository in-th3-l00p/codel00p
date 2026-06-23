//! Whole-file mutation tools: create, replace, delete, move, and copy.
//!
//! These operate on a file as a unit (unlike `super::apply_patch`, which edits
//! within one). They share the existence/destination guards in `super::support`
//! so boundary checks and error wording stay consistent.

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

use super::support::{
    ensure_dest_free, optional_bool, read_source_file, require_existing_file, tool_failed,
};

pub struct CreateFileTool;

#[async_trait]
impl Tool for CreateFileTool {
    fn name(&self) -> &str {
        "create_file"
    }

    fn description(&self) -> &str {
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
        if workspace.exists(path)? {
            return Err(tool_failed(
                self.name(),
                format!("file already exists: {path}"),
            ));
        }

        workspace.write(path, content)?;

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

    fn description(&self) -> &str {
        "Replace a UTF-8 file inside the workspace root. Prefer `apply_patch` for \
         targeted edits; use this only to replace an entire file."
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
        require_existing_file(workspace, self.name(), path)?;
        workspace.write(path, content)?;

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

    fn description(&self) -> &str {
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
        require_existing_file(workspace, self.name(), path)?;
        workspace.delete(path)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "deleted",
        })))
    }
}

pub struct MoveFileTool;

#[async_trait]
impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "move_file"
    }

    fn description(&self) -> &str {
        "Move (rename) a file inside the workspace root. Binary-safe and \
         workspace-bounded. Fails if `from` is missing or `to` already exists \
         unless `overwrite` is true. Implemented as copy-then-delete through the \
         execution backend."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["from", "to"],
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "overwrite": { "type": "boolean" }
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
        let from = required_string(self.name(), &input, "from")?;
        let to = required_string(self.name(), &input, "to")?;
        let overwrite = optional_bool(&input, "overwrite");

        let bytes = read_source_file(workspace, self.name(), from)?;
        ensure_dest_free(workspace, self.name(), to, overwrite)?;

        // copy-then-delete so the move flows through the same boundary-checked,
        // backend-aware write/delete path as the other editing tools.
        workspace.write(to, &bytes)?;
        workspace.delete(from)?;

        Ok(ToolResult::json(json!({
            "from": from,
            "to": to,
            "operation": "moved",
            "bytes_written": bytes.len(),
        })))
    }
}

pub struct CopyFileTool;

#[async_trait]
impl Tool for CopyFileTool {
    fn name(&self) -> &str {
        "copy_file"
    }

    fn description(&self) -> &str {
        "Copy a file inside the workspace root, leaving the source intact. \
         Binary-safe and workspace-bounded. Fails if `from` is missing or `to` \
         already exists unless `overwrite` is true."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["from", "to"],
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "overwrite": { "type": "boolean" }
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
        let from = required_string(self.name(), &input, "from")?;
        let to = required_string(self.name(), &input, "to")?;
        let overwrite = optional_bool(&input, "overwrite");

        let bytes = read_source_file(workspace, self.name(), from)?;
        ensure_dest_free(workspace, self.name(), to, overwrite)?;

        workspace.write(to, &bytes)?;

        Ok(ToolResult::json(json!({
            "from": from,
            "to": to,
            "operation": "copied",
            "bytes_written": bytes.len(),
        })))
    }
}
