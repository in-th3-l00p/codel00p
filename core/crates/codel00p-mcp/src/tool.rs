//! Harness tool adapter that exposes discovered MCP tools to agent turns.

use std::sync::Arc;

use async_trait::async_trait;
use codel00p_harness::{HarnessError, Tool, ToolRegistry, ToolResult, Workspace};
use codel00p_protocol::PermissionScope;
use serde_json::Value;

use crate::{
    McpClient, McpError, McpToolCall, McpToolDescriptor,
    notifications::mcp_notification_progress_parts,
};

pub struct McpTool {
    descriptor: McpToolDescriptor,
    client: Arc<dyn McpClient>,
}

impl McpTool {
    pub fn new<T>(descriptor: McpToolDescriptor, client: T) -> Self
    where
        T: McpClient + 'static,
    {
        Self {
            descriptor,
            client: Arc::new(client),
        }
    }
}

pub async fn discover_tool_registry<T>(client: T) -> Result<ToolRegistry, McpError>
where
    T: McpClient + Clone + 'static,
{
    let descriptors = client.list_tools().await?;
    let mut registry = ToolRegistry::new();
    for descriptor in descriptors {
        registry = registry.with_tool(McpTool::new(descriptor, client.clone()));
    }
    Ok(registry)
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        self.descriptor.harness_tool_name()
    }

    fn description(&self) -> &str {
        self.descriptor.description()
    }

    fn input_schema(&self) -> Value {
        self.descriptor.input_schema().clone()
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        self.descriptor.permission_scope()
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let output = self
            .client
            .call_tool(McpToolCall::new(
                self.descriptor.server_id(),
                self.descriptor.tool_name(),
                input,
            ))
            .await
            .map_err(|error| HarnessError::ToolFailed {
                name: self.name().to_string(),
                message: error.to_string(),
            })?;

        let mut result = ToolResult::json(output.content().clone());
        for notification in output.notifications() {
            let (phase, message) = mcp_notification_progress_parts(notification);
            result = result.with_progress(phase, message);
        }
        Ok(result)
    }
}
