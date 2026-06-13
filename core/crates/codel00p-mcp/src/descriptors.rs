//! Public MCP descriptors, resource/prompt payloads, and client trait contracts.

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{McpClientNotification, McpError};

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceTemplateDescriptor {
    server_id: String,
    uri_template: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

impl McpResourceTemplateDescriptor {
    pub fn new(
        server_id: impl Into<String>,
        uri_template: impl Into<String>,
        name: impl Into<String>,
        description: Option<impl Into<String>>,
        mime_type: Option<impl Into<String>>,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            uri_template: uri_template.into(),
            name: name.into(),
            description: description.map(Into::into),
            mime_type: mime_type.map(Into::into),
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn uri_template(&self) -> &str {
        &self.uri_template
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceContent {
    uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob: Option<String>,
}

impl McpResourceContent {
    pub fn new(
        uri: impl Into<String>,
        mime_type: Option<impl Into<String>>,
        text: Option<impl Into<String>>,
        blob: Option<impl Into<String>>,
    ) -> Self {
        Self {
            uri: uri.into(),
            mime_type: mime_type.map(Into::into),
            text: text.map(Into::into),
            blob: blob.map(Into::into),
        }
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn blob(&self) -> Option<&str> {
        self.blob.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceOutput {
    contents: Vec<McpResourceContent>,
}

impl McpResourceOutput {
    pub fn new(contents: Vec<McpResourceContent>) -> Self {
        Self { contents }
    }

    pub fn contents(&self) -> &[McpResourceContent] {
        &self.contents
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptArgument {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    required: bool,
}

impl McpPromptArgument {
    pub fn new(
        name: impl Into<String>,
        description: Option<impl Into<String>>,
        required: bool,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.map(Into::into),
            required,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn required(&self) -> bool {
        self.required
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptDescriptor {
    server_id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    arguments: Vec<McpPromptArgument>,
}

impl McpPromptDescriptor {
    pub fn new(
        server_id: impl Into<String>,
        name: impl Into<String>,
        description: Option<impl Into<String>>,
        arguments: Vec<McpPromptArgument>,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            name: name.into(),
            description: description.map(Into::into),
            arguments,
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn arguments(&self) -> &[McpPromptArgument] {
        &self.arguments
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpPromptMessage {
    role: String,
    content: Value,
}

impl McpPromptMessage {
    pub fn new(role: impl Into<String>, content: Value) -> Self {
        Self {
            role: role.into(),
            content,
        }
    }

    pub fn text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(
            role,
            serde_json::json!({
                "type": "text",
                "text": text.into()
            }),
        )
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn content(&self) -> &Value {
        &self.content
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpPromptOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    messages: Vec<McpPromptMessage>,
}

impl McpPromptOutput {
    pub fn new(description: Option<impl Into<String>>, messages: Vec<McpPromptMessage>) -> Self {
        Self {
            description: description.map(Into::into),
            messages,
        }
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn messages(&self) -> &[McpPromptMessage] {
        &self.messages
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notifications: Vec<McpClientNotification>,
}

impl McpToolOutput {
    pub fn json(content: Value) -> Self {
        Self {
            content,
            notifications: Vec::new(),
        }
    }

    pub fn with_notifications(mut self, notifications: Vec<McpClientNotification>) -> Self {
        self.notifications = notifications;
        self
    }

    pub fn content(&self) -> &Value {
        &self.content
    }

    pub fn notifications(&self) -> &[McpClientNotification] {
        &self.notifications
    }
}

#[async_trait]
pub trait McpClient: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<McpToolDescriptor>, McpError>;

    async fn list_resources(&self) -> Result<Vec<McpResourceDescriptor>, McpError>;

    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolOutput, McpError>;
}
