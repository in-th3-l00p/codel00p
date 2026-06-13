//! Streamable HTTP MCP client implementation.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::{
    JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, McpClient, McpClientExchange, McpError,
    McpInitialization, McpPromptDescriptor, McpPromptOutput, McpResourceDescriptor,
    McpResourceOutput, McpResourceTemplateDescriptor, McpToolCall, McpToolDescriptor,
    McpToolOutput,
    capabilities::client_capabilities,
    parsing::{
        client_exchange_from_messages, decode_sse_json_rpc_messages, json_rpc_result_from_response,
        mcp_list_params, mcp_next_cursor, parse_initialization_response, parse_prompt_descriptors,
        parse_prompt_output, parse_resource_descriptors, parse_resource_output,
        parse_resource_template_descriptors, parse_tool_descriptors,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpServerEndpoint {
    server_id: String,
    url: String,
    bearer_token: Option<String>,
    headers: Vec<(String, String)>,
    request_timeout: Duration,
}

impl HttpServerEndpoint {
    pub fn new(server_id: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            server_id: server_id.into(),
            url: url.into(),
            bearer_token: None,
            headers: Vec::new(),
            request_timeout: Duration::from_secs(30),
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn with_bearer_token(mut self, bearer_token: impl Into<String>) -> Self {
        self.bearer_token = Some(bearer_token.into());
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }
}

pub struct McpHttpClient {
    server_id: String,
    url: String,
    client: reqwest::Client,
    bearer_token: Option<String>,
    headers: Vec<(String, String)>,
    session_id: Option<String>,
    next_request_id: u64,
    initialized: Option<McpInitialization>,
}

impl McpHttpClient {
    pub fn connect(endpoint: HttpServerEndpoint) -> Result<Self, McpError> {
        let client = reqwest::Client::builder()
            .timeout(endpoint.request_timeout)
            .build()
            .map_err(|error| McpError::HttpTransport {
                server_id: endpoint.server_id.clone(),
                message: format!("failed to build http client: {error}"),
            })?;
        Ok(Self {
            server_id: endpoint.server_id,
            url: endpoint.url,
            client,
            bearer_token: endpoint.bearer_token,
            headers: endpoint.headers,
            session_id: None,
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
                    "capabilities": client_capabilities(&[]),
                    "clientInfo": {
                        "name": "codel00p",
                        "title": "codel00p",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;
        let initialization = parse_initialization_response(&self.server_id, response, |message| {
            McpError::HttpTransport {
                server_id: self.server_id.clone(),
                message,
            }
        })?;
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
        let exchange = self.post_json_rpc(&request).await?;
        let response =
            json_rpc_result_from_response(&self.server_id, exchange.response, |message| {
                McpError::HttpTransport {
                    server_id: self.server_id.clone(),
                    message,
                }
            })?;
        Ok(McpClientExchange {
            response,
            notifications: exchange.notifications,
        })
    }

    pub async fn notify(
        &mut self,
        method: impl Into<String>,
        params: Value,
    ) -> Result<(), McpError> {
        let notification = JsonRpcMessage::Notification(JsonRpcNotification::new(method, params));
        let exchange = self.post_json_rpc(&notification).await?;
        if exchange.response.is_null() {
            return Ok(());
        }
        json_rpc_result_from_response(&self.server_id, exchange.response, |message| {
            McpError::HttpTransport {
                server_id: self.server_id.clone(),
                message,
            }
        })?;
        Ok(())
    }

    async fn post_json_rpc(
        &mut self,
        message: &JsonRpcMessage,
    ) -> Result<McpClientExchange, McpError> {
        let mut request = self
            .client
            .post(&self.url)
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .json(message);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some(session_id) = &self.session_id {
            request = request.header("Mcp-Session-Id", session_id);
        }
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .send()
            .await
            .map_err(|error| self.http_error(format!("request failed: {error}")))?;
        if let Some(session_id) = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
        {
            self.session_id = Some(session_id.to_string());
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = response
            .text()
            .await
            .map_err(|error| self.http_error(format!("failed to read response body: {error}")))?;
        if !status.is_success() {
            return Err(self.http_error(format!("server returned HTTP {status}: {}", body.trim())));
        }
        if status == reqwest::StatusCode::ACCEPTED || body.trim().is_empty() {
            return Ok(McpClientExchange {
                response: Value::Null,
                notifications: Vec::new(),
            });
        }
        if content_type
            .to_ascii_lowercase()
            .starts_with("text/event-stream")
        {
            let messages =
                decode_sse_json_rpc_messages(&body).map_err(|message| self.http_error(message))?;
            return client_exchange_from_messages(messages, |message| self.http_error(message));
        }
        let response = serde_json::from_str(&body)
            .map_err(|error| self.http_error(format!("invalid json response: {error}")))?;
        Ok(McpClientExchange {
            response,
            notifications: Vec::new(),
        })
    }

    fn http_error(&self, message: impl Into<String>) -> McpError {
        McpError::HttpTransport {
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
                |message| McpError::HttpTransport {
                    server_id: self.server_id.clone(),
                    message,
                },
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
                |message| McpError::HttpTransport {
                    server_id: self.server_id.clone(),
                    message,
                },
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
                |message| McpError::HttpTransport {
                    server_id: self.server_id.clone(),
                    message,
                },
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
        parse_resource_output(response, |message| McpError::HttpTransport {
            server_id: self.server_id.clone(),
            message,
        })
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
                |message| McpError::HttpTransport {
                    server_id: self.server_id.clone(),
                    message,
                },
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
        parse_prompt_output(response, |message| McpError::HttpTransport {
            server_id: self.server_id.clone(),
            message,
        })
    }

    pub async fn call_tool(&mut self, call: McpToolCall) -> Result<McpToolOutput, McpError> {
        if call.server_id() != self.server_id {
            return Err(McpError::Tool {
                server_id: call.server_id().to_string(),
                tool_name: call.tool_name().to_string(),
                message: format!("http client is connected to `{}`", self.server_id),
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

#[async_trait]
impl McpClient for Arc<Mutex<McpHttpClient>> {
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
