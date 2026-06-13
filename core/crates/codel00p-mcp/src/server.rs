//! MCP server-side request dispatch helpers for stdio handlers.

use std::{
    collections::HashSet,
    io::{BufRead, Write},
};

use serde_json::Value;

use crate::McpError;

#[derive(Clone, Debug, PartialEq)]
pub struct McpServerResponse {
    result: Value,
    updated_resources: Vec<String>,
}

impl McpServerResponse {
    pub fn new(result: Value) -> Self {
        Self {
            result,
            updated_resources: Vec::new(),
        }
    }

    pub fn with_updated_resource(mut self, uri: impl Into<String>) -> Self {
        self.updated_resources.push(uri.into());
        self
    }

    pub fn result(&self) -> &Value {
        &self.result
    }

    pub fn updated_resources(&self) -> &[String] {
        &self.updated_resources
    }
}

pub trait McpServerHandler {
    fn handle_method(&mut self, method: &str, params: &Value) -> Result<McpServerResponse, String>;
}

#[derive(Default)]
pub struct McpServerRuntime {
    resource_subscriptions: HashSet<String>,
}

impl McpServerRuntime {
    pub fn handle_request_with_handler<H>(&mut self, request: Value, handler: &mut H) -> Vec<Value>
    where
        H: McpServerHandler,
    {
        self.handle_request(request, |method, params| {
            handler.handle_method(method, params)
        })
    }

    pub fn handle_request<F>(&mut self, request: Value, dispatch: F) -> Vec<Value>
    where
        F: FnOnce(&str, &Value) -> Result<McpServerResponse, String>,
    {
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(id) = request.get("id").cloned() else {
            return Vec::new();
        };
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let progress_token = progress_token(&params);

        let result = match method {
            "resources/subscribe" => self.subscribe_resource(&params),
            "resources/unsubscribe" => self.unsubscribe_resource(&params),
            _ => dispatch(method, &params),
        };

        match result {
            Ok(response) => {
                let mut messages = progress_notifications(progress_token.as_ref());
                messages.push(json_rpc_result(id, response.result));
                messages.extend(
                    response
                        .updated_resources
                        .into_iter()
                        .filter(|uri| self.resource_subscriptions.contains(uri))
                        .map(|uri| resource_updated_notification(&uri)),
                );
                messages
            }
            Err(message) => {
                let mut messages = progress_notifications(progress_token.as_ref());
                messages.push(json_rpc_error(id, message));
                messages
            }
        }
    }

    fn subscribe_resource(&mut self, params: &Value) -> Result<McpServerResponse, String> {
        let uri = required_param_string(params, "uri")?;
        self.resource_subscriptions.insert(uri.to_string());
        Ok(McpServerResponse::new(serde_json::json!({})))
    }

    fn unsubscribe_resource(&mut self, params: &Value) -> Result<McpServerResponse, String> {
        let uri = required_param_string(params, "uri")?;
        self.resource_subscriptions.remove(uri);
        Ok(McpServerResponse::new(serde_json::json!({})))
    }
}

pub fn serve_stdio_server<R, W, H>(
    reader: R,
    mut writer: W,
    handler: &mut H,
) -> Result<(), McpError>
where
    R: BufRead,
    W: Write,
    H: McpServerHandler,
{
    let mut runtime = McpServerRuntime::default();
    for line in reader.lines() {
        let line = line.map_err(|error| McpError::StdioTransport {
            server_id: "server".to_string(),
            message: error.to_string(),
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value =
            serde_json::from_str(&line).map_err(|error| McpError::InvalidStdioMessage {
                message: format!("invalid json-rpc request: {error}"),
            })?;
        for response in runtime.handle_request_with_handler(request, handler) {
            serde_json::to_writer(&mut writer, &response).map_err(|error| {
                McpError::StdioTransport {
                    server_id: "server".to_string(),
                    message: error.to_string(),
                }
            })?;
            writer
                .write_all(b"\n")
                .and_then(|_| writer.flush())
                .map_err(|error| McpError::StdioTransport {
                    server_id: "server".to_string(),
                    message: error.to_string(),
                })?;
        }
    }
    Ok(())
}

fn required_param_string<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required parameter `{key}`"))
}

fn progress_token(params: &Value) -> Option<Value> {
    let token = params.get("_meta")?.get("progressToken")?;
    if token.is_string() || token.is_number() {
        Some(token.clone())
    } else {
        None
    }
}

fn progress_notifications(progress_token: Option<&Value>) -> Vec<Value> {
    let Some(progress_token) = progress_token else {
        return Vec::new();
    };
    vec![
        progress_notification(progress_token, 1, "Processing request"),
        progress_notification(progress_token, 2, "Completing request"),
    ]
}

fn progress_notification(progress_token: &Value, progress: u64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {
            "progressToken": progress_token,
            "progress": progress,
            "total": 2,
            "message": message
        }
    })
}

fn resource_updated_notification(uri: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/resources/updated",
        "params": {
            "uri": uri
        }
    })
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn json_rpc_error(id: Value, message: String) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32000,
            "message": message
        }
    })
}
