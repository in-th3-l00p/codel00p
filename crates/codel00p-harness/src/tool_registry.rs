use std::{collections::BTreeMap, sync::Arc};

use codel00p_protocol::PermissionScope;
use serde_json::Value;

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{ListFilesTool, ReadFileTool, SearchTextTool, Tool},
    workspace::Workspace,
};

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<&'static str, Arc<dyn Tool>>,
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

    pub fn with_tool<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name(), Arc::new(tool));
        self
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.tools.keys().copied().collect()
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
