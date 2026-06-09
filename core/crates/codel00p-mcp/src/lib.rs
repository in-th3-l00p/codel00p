use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
};

use async_trait::async_trait;
use codel00p_harness::{HarnessError, Tool, ToolRegistry, ToolResult, Workspace};
use codel00p_protocol::PermissionScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{Child, ChildStdin, ChildStdout, Command},
};

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

    #[error("invalid stdio transport message: {message}")]
    InvalidStdioMessage { message: String },

    #[error("mcp stdio transport failed for {server_id}: {message}")]
    StdioTransport { server_id: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(u64),
    String(String),
}

impl From<u64> for JsonRpcId {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for JsonRpcId {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    jsonrpc: String,
    id: JsonRpcId,
    method: String,
    params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<JsonRpcId>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    jsonrpc: String,
    id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

impl JsonRpcResponse {
    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub fn error(&self) -> Option<&Value> {
        self.error.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
    Response(JsonRpcResponse),
    Raw(Value),
}

impl JsonRpcMessage {
    pub fn request(request: JsonRpcRequest) -> Self {
        Self::Request(request)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    params: Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct McpInitialization {
    protocol_version: String,
    capabilities: Value,
    server_info: Value,
    instructions: Option<String>,
}

impl McpInitialization {
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    pub fn server_info(&self) -> &Value {
        &self.server_info
    }

    pub fn server_name(&self) -> Option<&str> {
        self.server_info.get("name").and_then(Value::as_str)
    }

    pub fn server_version(&self) -> Option<&str> {
        self.server_info.get("version").and_then(Value::as_str)
    }

    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdioServerCommand {
    server_id: String,
    program: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
}

impl StdioServerCommand {
    pub fn new<I, S>(server_id: impl Into<String>, program: impl Into<PathBuf>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Self {
            server_id: server_id.into(),
            program: program.into(),
            args: args
                .into_iter()
                .map(|arg| arg.as_ref().to_os_string())
                .collect(),
            env: Vec::new(),
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn with_env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }
}

pub struct McpStdioClient {
    server_id: String,
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
    initialized: Option<McpInitialization>,
}

impl McpStdioClient {
    pub async fn spawn(command: StdioServerCommand) -> Result<Self, McpError> {
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .envs(command.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| McpError::StdioTransport {
                server_id: command.server_id.clone(),
                message: format!("failed to spawn server: {error}"),
            })?;
        let stdin = child.stdin.take().ok_or_else(|| McpError::StdioTransport {
            server_id: command.server_id.clone(),
            message: "server stdin was not available".to_string(),
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::StdioTransport {
                server_id: command.server_id.clone(),
                message: "server stdout was not available".to_string(),
            })?;

        Ok(Self {
            server_id: command.server_id,
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            next_request_id: 1,
            initialized: None,
        })
    }

    pub async fn initialize(&mut self) -> Result<McpInitialization, McpError> {
        let response = self
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "codel00p",
                        "title": "codel00p",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;
        let protocol_version = response
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| self.stdio_error("initialize response omitted protocolVersion"))?
            .to_string();
        let initialization = McpInitialization {
            protocol_version,
            capabilities: response
                .get("capabilities")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            server_info: response
                .get("serverInfo")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            instructions: response
                .get("instructions")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        };
        self.notify(
            "notifications/initialized",
            Value::Object(Default::default()),
        )
        .await?;
        self.initialized = Some(initialization.clone());
        Ok(initialization)
    }

    pub async fn request(
        &mut self,
        method: impl Into<String>,
        params: Value,
    ) -> Result<Value, McpError> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        let request = JsonRpcMessage::request(JsonRpcRequest::new(request_id, method, params));
        let encoded = encode_stdio_message(&request)?;
        self.stdin
            .write_all(encoded.as_bytes())
            .await
            .map_err(|error| self.stdio_error(format!("failed to write request: {error}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| self.stdio_error(format!("failed to flush request: {error}")))?;

        let mut line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut line)
            .await
            .map_err(|error| self.stdio_error(format!("failed to read response: {error}")))?;
        if bytes == 0 {
            return Err(self.stdio_error("server closed stdout before responding"));
        }

        match decode_stdio_message(&line)? {
            JsonRpcMessage::Response(response) => {
                if let Some(error) = response.error() {
                    return Err(self.stdio_error(format!("json-rpc error response: {error}")));
                }
                response
                    .result()
                    .cloned()
                    .ok_or_else(|| self.stdio_error("json-rpc response omitted result"))
            }
            JsonRpcMessage::Raw(value) => value
                .get("result")
                .cloned()
                .ok_or_else(|| self.stdio_error("json-rpc response omitted result")),
            JsonRpcMessage::Request(_) | JsonRpcMessage::Notification(_) => {
                Err(self.stdio_error("server returned a non-response message"))
            }
        }
    }

    pub async fn notify(
        &mut self,
        method: impl Into<String>,
        params: Value,
    ) -> Result<(), McpError> {
        let notification = JsonRpcMessage::Notification(JsonRpcNotification::new(method, params));
        let encoded = encode_stdio_message(&notification)?;
        self.stdin
            .write_all(encoded.as_bytes())
            .await
            .map_err(|error| self.stdio_error(format!("failed to write notification: {error}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| self.stdio_error(format!("failed to flush notification: {error}")))?;
        Ok(())
    }

    fn stdio_error(&self, message: impl Into<String>) -> McpError {
        McpError::StdioTransport {
            server_id: self.server_id.clone(),
            message: message.into(),
        }
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDescriptor>, McpError> {
        let response = self
            .request("tools/list", Value::Object(Default::default()))
            .await?;
        let tools = response
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| self.stdio_error("tools/list response omitted tools array"))?;

        tools
            .iter()
            .map(|tool| {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.stdio_error("tool descriptor omitted name"))?;
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP tool.");
                let input_schema = tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({ "type": "object" }));
                Ok(McpToolDescriptor::new(
                    self.server_id.clone(),
                    name,
                    description,
                    input_schema,
                ))
            })
            .collect()
    }

    pub async fn list_resources(&mut self) -> Result<Vec<McpResourceDescriptor>, McpError> {
        let response = self
            .request("resources/list", Value::Object(Default::default()))
            .await?;
        let resources = response
            .get("resources")
            .and_then(Value::as_array)
            .ok_or_else(|| self.stdio_error("resources/list response omitted resources array"))?;

        resources
            .iter()
            .map(|resource| {
                let uri = resource
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.stdio_error("resource descriptor omitted uri"))?;
                let name = resource.get("name").and_then(Value::as_str).unwrap_or(uri);
                let mime_type = resource.get("mimeType").and_then(Value::as_str);
                Ok(McpResourceDescriptor::new(
                    self.server_id.clone(),
                    uri,
                    name,
                    mime_type,
                ))
            })
            .collect()
    }

    pub async fn call_tool(&mut self, call: McpToolCall) -> Result<McpToolOutput, McpError> {
        if call.server_id() != self.server_id {
            return Err(McpError::Tool {
                server_id: call.server_id().to_string(),
                tool_name: call.tool_name().to_string(),
                message: format!("stdio client is connected to `{}`", self.server_id),
            });
        }

        let response = self
            .request(
                "tools/call",
                serde_json::json!({
                    "name": call.tool_name(),
                    "arguments": call.input(),
                }),
            )
            .await?;

        Ok(McpToolOutput::json(response))
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub fn encode_stdio_message(message: &JsonRpcMessage) -> Result<String, McpError> {
    let encoded =
        serde_json::to_string(message).map_err(|error| McpError::InvalidStdioMessage {
            message: error.to_string(),
        })?;
    if encoded.contains('\n') || encoded.contains('\r') {
        return Err(McpError::InvalidStdioMessage {
            message: "stdio messages must not contain embedded newlines".to_string(),
        });
    }
    Ok(format!("{encoded}\n"))
}

pub fn decode_stdio_message(line: &str) -> Result<JsonRpcMessage, McpError> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(McpError::InvalidStdioMessage {
            message: "stdio messages must not contain embedded newlines".to_string(),
        });
    }
    serde_json::from_str(trimmed).map_err(|error| McpError::InvalidStdioMessage {
        message: error.to_string(),
    })
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
