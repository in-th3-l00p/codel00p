//! Stdio MCP process client, root handling, and newline-delimited transport loop.

use std::{process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::timeout,
};

use crate::{
    JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpClient,
    McpClientExchange, McpClientNotification, McpError, McpInitialization, McpPromptDescriptor,
    McpPromptOutput, McpResourceDescriptor, McpResourceOutput, McpResourceTemplateDescriptor,
    McpToolCall, McpToolDescriptor, McpToolOutput,
    capabilities::client_capabilities,
    notifications::client_notification_from_json_rpc,
    parsing::{
        mcp_list_params, mcp_next_cursor, parse_prompt_descriptors, parse_prompt_output,
        parse_resource_descriptors, parse_resource_output, parse_resource_template_descriptors,
        parse_tool_descriptors,
    },
    stdio_codec::{decode_stdio_message, encode_stdio_message},
};

mod command;

pub use command::{McpClientRoot, StdioServerCommand};

pub struct McpStdioClient {
    server_id: String,
    child: Child,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
    initialized: Option<McpInitialization>,
    roots: Vec<McpClientRoot>,
    request_timeout: Duration,
    shutdown_timeout: Duration,
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
            stdin: Some(BufWriter::new(stdin)),
            stdout: BufReader::new(stdout),
            next_request_id: 1,
            initialized: None,
            roots: command.roots,
            request_timeout: command.request_timeout,
            shutdown_timeout: command.shutdown_timeout,
        })
    }

    pub async fn initialize(&mut self) -> Result<McpInitialization, McpError> {
        let response = self
            .request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": client_capabilities(&self.roots),
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
        Ok(self.request_exchange(method, params).await?.response)
    }

    async fn request_exchange(
        &mut self,
        method: impl Into<String>,
        params: Value,
    ) -> Result<McpClientExchange, McpError> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        let request = JsonRpcMessage::request(JsonRpcRequest::new(request_id, method, params));
        self.write_message(&request).await?;

        let mut notifications = Vec::new();
        loop {
            let mut line = String::new();
            let bytes = timeout(self.request_timeout, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| {
                    self.stdio_error(format!(
                        "request timed out after {} ms",
                        self.request_timeout.as_millis()
                    ))
                })?
                .map_err(|error| self.stdio_error(format!("failed to read response: {error}")))?;
            if bytes == 0 {
                return Err(self.stdio_error("server closed stdout before responding"));
            }

            match decode_stdio_message(&line)? {
                JsonRpcMessage::Response(response) => {
                    if let Some(error) = response.error() {
                        return Err(self.stdio_error(format!("json-rpc error response: {error}")));
                    }
                    let response = response
                        .result()
                        .cloned()
                        .ok_or_else(|| self.stdio_error("json-rpc response omitted result"))?;
                    return Ok(McpClientExchange {
                        response,
                        notifications,
                    });
                }
                JsonRpcMessage::Raw(value) => {
                    let response = value
                        .get("result")
                        .cloned()
                        .ok_or_else(|| self.stdio_error("json-rpc response omitted result"))?;
                    return Ok(McpClientExchange {
                        response,
                        notifications,
                    });
                }
                JsonRpcMessage::Notification(notification) => {
                    notifications.push(client_notification_from_json_rpc(notification));
                }
                JsonRpcMessage::Request(request) => {
                    self.respond_to_server_request(request).await?;
                }
            }
        }
    }

    pub async fn subscribe_resource(&mut self, uri: impl Into<String>) -> Result<(), McpError> {
        self.request(
            "resources/subscribe",
            serde_json::json!({
                "uri": uri.into()
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn unsubscribe_resource(&mut self, uri: impl Into<String>) -> Result<(), McpError> {
        self.request(
            "resources/unsubscribe",
            serde_json::json!({
                "uri": uri.into()
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn read_notification(&mut self) -> Result<McpClientNotification, McpError> {
        loop {
            let mut line = String::new();
            let bytes = timeout(self.request_timeout, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| {
                    self.stdio_error(format!(
                        "notification read timed out after {} ms",
                        self.request_timeout.as_millis()
                    ))
                })?
                .map_err(|error| {
                    self.stdio_error(format!("failed to read notification: {error}"))
                })?;
            if bytes == 0 {
                return Err(self.stdio_error("server closed stdout before notification"));
            }
            if line.trim().is_empty() {
                continue;
            }
            return match decode_stdio_message(&line)? {
                JsonRpcMessage::Notification(notification) => {
                    Ok(client_notification_from_json_rpc(notification))
                }
                JsonRpcMessage::Request(request) => {
                    self.respond_to_server_request(request).await?;
                    continue;
                }
                JsonRpcMessage::Response(_) | JsonRpcMessage::Raw(_) => {
                    Err(self.stdio_error("server returned a non-notification message"))
                }
            };
        }
    }

    pub async fn notify(
        &mut self,
        method: impl Into<String>,
        params: Value,
    ) -> Result<(), McpError> {
        let notification = JsonRpcMessage::Notification(JsonRpcNotification::new(method, params));
        let encoded = encode_stdio_message(&notification)?;
        let server_id = self.server_id.clone();
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::StdioTransport {
                server_id,
                message: "server stdin is closed".to_string(),
            })?;
        stdin
            .write_all(encoded.as_bytes())
            .await
            .map_err(|error| self.stdio_error(format!("failed to write notification: {error}")))?;
        let server_id = self.server_id.clone();
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::StdioTransport {
                server_id,
                message: "server stdin is closed".to_string(),
            })?;
        stdin
            .flush()
            .await
            .map_err(|error| self.stdio_error(format!("failed to flush notification: {error}")))?;
        Ok(())
    }

    async fn respond_to_server_request(&mut self, request: JsonRpcRequest) -> Result<(), McpError> {
        let response = match request.method() {
            "roots/list" => JsonRpcMessage::Response(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: Some(serde_json::json!({
                    "roots": self.roots.clone()
                })),
                error: None,
            }),
            method => JsonRpcMessage::Response(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: None,
                error: Some(serde_json::json!({
                    "code": -32601,
                    "message": format!("client request method is not supported: {method}")
                })),
            }),
        };
        self.write_message(&response).await
    }

    async fn write_message(&mut self, message: &JsonRpcMessage) -> Result<(), McpError> {
        let encoded = encode_stdio_message(message)?;
        let server_id = self.server_id.clone();
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::StdioTransport {
                server_id,
                message: "server stdin is closed".to_string(),
            })?;
        stdin
            .write_all(encoded.as_bytes())
            .await
            .map_err(|error| self.stdio_error(format!("failed to write message: {error}")))?;
        let server_id = self.server_id.clone();
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::StdioTransport {
                server_id,
                message: "server stdin is closed".to_string(),
            })?;
        stdin
            .flush()
            .await
            .map_err(|error| self.stdio_error(format!("failed to flush message: {error}")))?;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), McpError> {
        self.stdin.take();
        match timeout(self.shutdown_timeout, self.child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(error)) => {
                Err(self.stdio_error(format!("failed to wait for shutdown: {error}")))
            }
            Err(_) => {
                self.child
                    .start_kill()
                    .map_err(|error| self.stdio_error(format!("failed to kill server: {error}")))?;
                let _ = self.child.wait().await;
                Err(self.stdio_error(format!(
                    "server did not exit within {} ms",
                    self.shutdown_timeout.as_millis()
                )))
            }
        }
    }

    fn stdio_error(&self, message: impl Into<String>) -> McpError {
        McpError::StdioTransport {
            server_id: self.server_id.clone(),
            message: message.into(),
        }
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDescriptor>, McpError> {
        let mut cursor = None;
        let mut descriptors = Vec::new();
        loop {
            let response = self
                .request("tools/list", mcp_list_params(cursor.as_deref()))
                .await?;
            descriptors.extend(parse_tool_descriptors(
                &self.server_id,
                response.clone(),
                |message| self.stdio_error(message),
            )?);
            let Some(next_cursor) = mcp_next_cursor(&response) else {
                return Ok(descriptors);
            };
            cursor = Some(next_cursor);
        }
    }

    pub async fn list_resources(&mut self) -> Result<Vec<McpResourceDescriptor>, McpError> {
        let mut cursor = None;
        let mut descriptors = Vec::new();
        loop {
            let response = self
                .request("resources/list", mcp_list_params(cursor.as_deref()))
                .await?;
            descriptors.extend(parse_resource_descriptors(
                &self.server_id,
                response.clone(),
                |message| self.stdio_error(message),
            )?);
            let Some(next_cursor) = mcp_next_cursor(&response) else {
                return Ok(descriptors);
            };
            cursor = Some(next_cursor);
        }
    }

    pub async fn list_resource_templates(
        &mut self,
    ) -> Result<Vec<McpResourceTemplateDescriptor>, McpError> {
        let mut cursor = None;
        let mut descriptors = Vec::new();
        loop {
            let response = self
                .request(
                    "resources/templates/list",
                    mcp_list_params(cursor.as_deref()),
                )
                .await?;
            descriptors.extend(parse_resource_template_descriptors(
                &self.server_id,
                response.clone(),
                |message| self.stdio_error(message),
            )?);
            let Some(next_cursor) = mcp_next_cursor(&response) else {
                return Ok(descriptors);
            };
            cursor = Some(next_cursor);
        }
    }

    pub async fn read_resource(
        &mut self,
        uri: impl Into<String>,
    ) -> Result<McpResourceOutput, McpError> {
        let response = self
            .request(
                "resources/read",
                serde_json::json!({
                    "uri": uri.into()
                }),
            )
            .await?;
        parse_resource_output(response, |message| self.stdio_error(message))
    }

    pub async fn set_logging_level(&mut self, level: impl Into<String>) -> Result<(), McpError> {
        self.request(
            "logging/setLevel",
            serde_json::json!({
                "level": level.into()
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn list_prompts(&mut self) -> Result<Vec<McpPromptDescriptor>, McpError> {
        let mut cursor = None;
        let mut descriptors = Vec::new();
        loop {
            let response = self
                .request("prompts/list", mcp_list_params(cursor.as_deref()))
                .await?;
            descriptors.extend(parse_prompt_descriptors(
                &self.server_id,
                response.clone(),
                |message| self.stdio_error(message),
            )?);
            let Some(next_cursor) = mcp_next_cursor(&response) else {
                return Ok(descriptors);
            };
            cursor = Some(next_cursor);
        }
    }

    pub async fn get_prompt(
        &mut self,
        name: impl Into<String>,
        arguments: Value,
    ) -> Result<McpPromptOutput, McpError> {
        let response = self
            .request(
                "prompts/get",
                serde_json::json!({
                    "name": name.into(),
                    "arguments": arguments
                }),
            )
            .await?;
        parse_prompt_output(response, |message| self.stdio_error(message))
    }

    pub async fn call_tool(&mut self, call: McpToolCall) -> Result<McpToolOutput, McpError> {
        if call.server_id() != self.server_id {
            return Err(McpError::Tool {
                server_id: call.server_id().to_string(),
                tool_name: call.tool_name().to_string(),
                message: format!("stdio client is connected to `{}`", self.server_id),
            });
        }

        let exchange = self
            .request_exchange(
                "tools/call",
                serde_json::json!({
                    "name": call.tool_name(),
                    "arguments": call.input(),
                }),
            )
            .await?;

        Ok(McpToolOutput::json(exchange.response).with_notifications(exchange.notifications))
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[async_trait]
impl McpClient for Arc<Mutex<McpStdioClient>> {
    async fn list_tools(&self) -> Result<Vec<McpToolDescriptor>, McpError> {
        self.lock().await.list_tools().await
    }

    async fn list_resources(&self) -> Result<Vec<McpResourceDescriptor>, McpError> {
        self.lock().await.list_resources().await
    }

    async fn call_tool(&self, call: McpToolCall) -> Result<McpToolOutput, McpError> {
        self.lock().await.call_tool(call).await
    }
}
