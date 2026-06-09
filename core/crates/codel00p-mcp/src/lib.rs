use std::sync::Arc;

use async_trait::async_trait;
use codel00p_harness::{HarnessError, Tool, ToolRegistry, ToolResult, Workspace};
use codel00p_protocol::PermissionScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("mcp client failed: {message}")]
    Client { message: String },

    #[error("mcp tool failed: {server_id}/{tool_name}: {message}")]
    Tool {
        server_id: String,
        tool_name: String,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpToolDescriptor {
    server_id: String,
    tool_name: String,
    harness_tool_name: String,
    description: String,
    input_schema: Value,
    permission_scope: PermissionScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceDescriptor {
    server_id: String,
    uri: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

impl McpResourceDescriptor {
    pub fn new(
        server_id: impl Into<String>,
        uri: impl Into<String>,
        name: impl Into<String>,
        mime_type: Option<impl Into<String>>,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            uri: uri.into(),
            name: name.into(),
            mime_type: mime_type.map(Into::into),
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

impl McpToolDescriptor {
    pub fn new(
        server_id: impl Into<String>,
        tool_name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        let server_id = server_id.into();
        let tool_name = tool_name.into();
        let harness_tool_name = format!("mcp.{server_id}.{tool_name}");
        Self {
            server_id,
            tool_name,
            harness_tool_name,
            description: description.into(),
            input_schema,
            permission_scope: PermissionScope::ExternalConnector,
        }
    }

    pub fn with_permission_scope(mut self, permission_scope: PermissionScope) -> Self {
        self.permission_scope = permission_scope;
        self
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn harness_tool_name(&self) -> &str {
        &self.harness_tool_name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    pub fn permission_scope(&self) -> PermissionScope {
        self.permission_scope
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpToolCall {
    server_id: String,
    tool_name: String,
    input: Value,
}

impl McpToolCall {
    pub fn new(server_id: impl Into<String>, tool_name: impl Into<String>, input: Value) -> Self {
        Self {
            server_id: server_id.into(),
            tool_name: tool_name.into(),
            input,
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn input(&self) -> &Value {
        &self.input
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpToolOutput {
    content: Value,
}

impl McpToolOutput {
    pub fn json(content: Value) -> Self {
        Self { content }
    }

    pub fn content(&self) -> &Value {
        &self.content
    }
}

#[async_trait]
pub trait McpClient: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<McpToolDescriptor>, McpError>;

    async fn list_resources(&self) -> Result<Vec<McpResourceDescriptor>, McpError>;

    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolOutput, McpError>;
}

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

        Ok(ToolResult::json(output.content().clone()))
    }
}

pub fn crate_name() -> &'static str {
    "codel00p-mcp"
}
