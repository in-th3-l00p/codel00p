use std::{collections::BTreeMap, sync::Arc};

use serde_json::Value;

use crate::{errors::HarnessError, tool_result::ToolResult, tools::Tool, workspace::Workspace};

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<&'static str, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
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
