//! Parsers for MCP JSON-RPC responses, descriptors, resources, prompts, and SSE frames.

use serde_json::Value;

use crate::{
    JsonRpcMessage, McpClientExchange, McpError, McpInitialization, McpPromptArgument,
    McpPromptDescriptor, McpPromptMessage, McpPromptOutput, McpResourceContent,
    McpResourceDescriptor, McpResourceOutput, McpResourceTemplateDescriptor, McpToolDescriptor,
    notifications::client_notification_from_json_rpc,
};

pub(crate) fn json_rpc_result_from_response<F>(
    server_id: &str,
    response: Value,
    error: F,
) -> Result<Value, McpError>
where
    F: Fn(String) -> McpError,
{
    match serde_json::from_value::<JsonRpcMessage>(response.clone()) {
        Ok(JsonRpcMessage::Response(response)) => {
            if let Some(error_value) = response.error() {
                return Err(error(format!("json-rpc error response: {error_value}")));
            }
            response
                .result()
                .cloned()
                .ok_or_else(|| error("json-rpc response omitted result".to_string()))
        }
        Ok(JsonRpcMessage::Raw(value)) => value
            .get("result")
            .cloned()
            .ok_or_else(|| error("json-rpc response omitted result".to_string())),
        Ok(JsonRpcMessage::Request(_) | JsonRpcMessage::Notification(_)) => {
            Err(error("server returned a non-response message".to_string()))
        }
        Err(_) => response
            .get("result")
            .cloned()
            .ok_or_else(|| error(format!("{server_id} response omitted result"))),
    }
}

pub(crate) fn parse_initialization_response<F>(
    server_id: &str,
    response: Value,
    error: F,
) -> Result<McpInitialization, McpError>
where
    F: Fn(String) -> McpError,
{
    let protocol_version = response
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            error(format!(
                "{server_id} initialize response omitted protocolVersion"
            ))
        })?
        .to_string();
    Ok(McpInitialization {
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
    })
}

pub(crate) fn mcp_list_params(cursor: Option<&str>) -> Value {
    match cursor {
        Some(cursor) => serde_json::json!({ "cursor": cursor }),
        None => Value::Object(Default::default()),
    }
}

pub(crate) fn mcp_next_cursor(response: &Value) -> Option<String> {
    response
        .get("nextCursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn parse_tool_descriptors<F>(
    server_id: &str,
    response: Value,
    error: F,
) -> Result<Vec<McpToolDescriptor>, McpError>
where
    F: Fn(String) -> McpError,
{
    let tools = response
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| error("tools/list response omitted tools array".to_string()))?;

    tools
        .iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| error("tool descriptor omitted name".to_string()))?;
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("MCP tool.");
            let input_schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "type": "object" }));
            Ok(McpToolDescriptor::new(
                server_id.to_string(),
                name,
                description,
                input_schema,
            ))
        })
        .collect()
}

pub(crate) fn parse_resource_descriptors<F>(
    server_id: &str,
    response: Value,
    error: F,
) -> Result<Vec<McpResourceDescriptor>, McpError>
where
    F: Fn(String) -> McpError,
{
    let resources = response
        .get("resources")
        .and_then(Value::as_array)
        .ok_or_else(|| error("resources/list response omitted resources array".to_string()))?;

    resources
        .iter()
        .map(|resource| {
            let uri = resource
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| error("resource descriptor omitted uri".to_string()))?;
            let name = resource.get("name").and_then(Value::as_str).unwrap_or(uri);
            let mime_type = resource.get("mimeType").and_then(Value::as_str);
            Ok(McpResourceDescriptor::new(
                server_id.to_string(),
                uri,
                name,
                mime_type,
            ))
        })
        .collect()
}

pub(crate) fn parse_resource_template_descriptors<F>(
    server_id: &str,
    response: Value,
    error: F,
) -> Result<Vec<McpResourceTemplateDescriptor>, McpError>
where
    F: Fn(String) -> McpError,
{
    let templates = response
        .get("resourceTemplates")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            error("resources/templates/list response omitted resourceTemplates array".to_string())
        })?;

    templates
        .iter()
        .map(|template| {
            let uri_template = template
                .get("uriTemplate")
                .and_then(Value::as_str)
                .ok_or_else(|| error("resource template omitted uriTemplate".to_string()))?;
            let name = template
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(uri_template);
            let description = template.get("description").and_then(Value::as_str);
            let mime_type = template.get("mimeType").and_then(Value::as_str);
            Ok(McpResourceTemplateDescriptor::new(
                server_id.to_string(),
                uri_template,
                name,
                description,
                mime_type,
            ))
        })
        .collect()
}

pub(crate) fn parse_resource_output<F>(
    response: Value,
    error: F,
) -> Result<McpResourceOutput, McpError>
where
    F: Fn(String) -> McpError,
{
    let contents = response
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(|| error("resources/read response omitted contents array".to_string()))?;
    let contents = contents
        .iter()
        .map(|content| {
            let uri = content
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| error("resource content omitted uri".to_string()))?;
            Ok(McpResourceContent::new(
                uri,
                content.get("mimeType").and_then(Value::as_str),
                content.get("text").and_then(Value::as_str),
                content.get("blob").and_then(Value::as_str),
            ))
        })
        .collect::<Result<Vec<_>, McpError>>()?;
    Ok(McpResourceOutput::new(contents))
}

pub(crate) fn parse_prompt_descriptors<F>(
    server_id: &str,
    response: Value,
    error: F,
) -> Result<Vec<McpPromptDescriptor>, McpError>
where
    F: Fn(String) -> McpError,
{
    let prompts = response
        .get("prompts")
        .and_then(Value::as_array)
        .ok_or_else(|| error("prompts/list response omitted prompts array".to_string()))?;

    prompts
        .iter()
        .map(|prompt| {
            let name = prompt
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| error("prompt descriptor omitted name".to_string()))?;
            let description = prompt.get("description").and_then(Value::as_str);
            let arguments = prompt
                .get("arguments")
                .and_then(Value::as_array)
                .map(|arguments| {
                    arguments
                        .iter()
                        .map(|argument| {
                            let name = argument
                                .get("name")
                                .and_then(Value::as_str)
                                .ok_or_else(|| error("prompt argument omitted name".to_string()))?;
                            let description = argument.get("description").and_then(Value::as_str);
                            let required = argument
                                .get("required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            Ok(McpPromptArgument::new(name, description, required))
                        })
                        .collect::<Result<Vec<_>, McpError>>()
                })
                .transpose()?
                .unwrap_or_default();
            Ok(McpPromptDescriptor::new(
                server_id.to_string(),
                name,
                description,
                arguments,
            ))
        })
        .collect()
}

pub(crate) fn parse_prompt_output<F>(response: Value, error: F) -> Result<McpPromptOutput, McpError>
where
    F: Fn(String) -> McpError,
{
    let description = response.get("description").and_then(Value::as_str);
    let messages = response
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| error("prompts/get response omitted messages array".to_string()))?;
    let messages = messages
        .iter()
        .map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| error("prompt message omitted role".to_string()))?;
            let content = message
                .get("content")
                .cloned()
                .ok_or_else(|| error("prompt message omitted content".to_string()))?;
            Ok(McpPromptMessage::new(role, content))
        })
        .collect::<Result<Vec<_>, McpError>>()?;
    Ok(McpPromptOutput::new(description, messages))
}

pub(crate) fn decode_sse_json_rpc_messages(body: &str) -> Result<Vec<Value>, String> {
    let mut messages = Vec::new();
    let mut data = String::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if !data.trim().is_empty() {
                messages.push(
                    serde_json::from_str(&data)
                        .map_err(|error| format!("invalid sse json response: {error}"))?,
                );
                data.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim_start());
        }
    }
    if !data.trim().is_empty() {
        messages.push(
            serde_json::from_str(&data)
                .map_err(|error| format!("invalid sse json response: {error}"))?,
        );
    }
    if messages.is_empty() {
        return Err("sse response omitted data event".to_string());
    }
    Ok(messages)
}

pub(crate) fn client_exchange_from_messages<F>(
    messages: Vec<Value>,
    error: F,
) -> Result<McpClientExchange, McpError>
where
    F: Fn(String) -> McpError,
{
    let mut notifications = Vec::new();
    for message in messages {
        match serde_json::from_value::<JsonRpcMessage>(message.clone()) {
            Ok(JsonRpcMessage::Notification(notification)) => {
                notifications.push(client_notification_from_json_rpc(notification));
            }
            Ok(JsonRpcMessage::Response(_)) | Ok(JsonRpcMessage::Raw(_)) => {
                return Ok(McpClientExchange {
                    response: message,
                    notifications,
                });
            }
            Ok(JsonRpcMessage::Request(_)) => {
                return Err(error(
                    "server returned a request while awaiting response".to_string(),
                ));
            }
            Err(_) => {
                if message.get("id").is_some() || message.get("result").is_some() {
                    return Ok(McpClientExchange {
                        response: message,
                        notifications,
                    });
                }
                return Err(error(
                    "server returned an invalid json-rpc message".to_string(),
                ));
            }
        }
    }
    Err(error("server omitted json-rpc response".to_string()))
}
