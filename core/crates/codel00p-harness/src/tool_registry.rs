use std::{collections::BTreeMap, sync::Arc};

use codel00p_protocol::PermissionScope;
use serde_json::Value;

use crate::{
    commands::RunCommandTool,
    editing::{ApplyPatchTool, CreateFileTool, DeleteFileTool, UpdateFileTool},
    errors::HarnessError,
    git::{GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool},
    tool_result::ToolResult,
    tools::{ListFilesTool, ReadFileTool, SearchTextTool, Tool, ToolSpec},
    web::web_tools,
    workspace::Workspace,
};

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_only_defaults() -> Self {
        Self::new()
            .with_tool(ListFilesTool)
            .with_tool(ReadFileTool)
            .with_tool(SearchTextTool)
    }

    pub fn editing_defaults() -> Self {
        Self::new()
            .with_tool(ApplyPatchTool)
            .with_tool(CreateFileTool)
            .with_tool(DeleteFileTool)
            .with_tool(UpdateFileTool)
    }

    pub fn command_defaults() -> Self {
        Self::new().with_tool(RunCommandTool)
    }

    pub fn git_defaults() -> Self {
        Self::new()
            .with_tool(GitCommitTool)
            .with_tool(GitDiffTool)
            .with_tool(GitLogTool)
            .with_tool(GitStatusTool)
    }

    /// Web tools (`web_fetch`, `web_search`) gated behind `PermissionScope::Network`.
    pub fn web_defaults() -> Self {
        Self::new().with_registry(web_tools())
    }

    pub fn with_tool<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
        self
    }

    /// Insert an already type-erased tool.
    ///
    /// This is the entry point used when tools are contributed dynamically (for
    /// example by a plugin) rather than by a statically typed `with_tool` call.
    /// A later insertion with the same tool name replaces an earlier one, so
    /// callers that fold several sources in sequence get last-writer-wins.
    pub fn with_tool_arc(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    pub fn with_registry(mut self, registry: Self) -> Self {
        self.tools.extend(registry.tools);
        self
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Model-facing definitions (name, description, JSON Schema) for every
    /// registered tool, in stable name order. This is what the provider request
    /// advertises so the model sees real parameters.
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools
            .values()
            .map(|tool| ToolSpec::from_tool(tool.as_ref()))
            .collect()
    }

    pub fn is_concurrency_safe(&self, name: &str, input: &Value) -> bool {
        self.tools
            .get(name)
            .map(|tool| tool.is_concurrency_safe(input))
            .unwrap_or(false)
    }

    pub fn permission_scope(&self, name: &str, input: &Value) -> PermissionScope {
        self.tools
            .get(name)
            .map(|tool| tool.permission_scope(input))
            .unwrap_or(PermissionScope::ExternalConnector)
    }

    pub async fn execute(
        &self,
        name: &str,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| HarnessError::ToolNotFound {
                name: name.to_string(),
            })?;

        tool.execute(workspace, input).await
    }
}
