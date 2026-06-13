//! Client-side MCP notifications and conversions to harness progress state.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::json_rpc::JsonRpcNotification;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpClientNotification {
    Progress {
        progress_token: Value,
        progress: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        total: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    ResourceUpdated {
        uri: String,
    },
    ToolsListChanged,
    ResourcesListChanged,
    PromptsListChanged,
    LogMessage {
        level: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logger: Option<String>,
        data: Value,
    },
    Other {
        method: String,
        params: Value,
    },
}

impl McpClientNotification {
    pub fn progress(
        progress_token: Value,
        progress: f64,
        total: Option<f64>,
        message: Option<impl Into<String>>,
    ) -> Self {
        Self::Progress {
            progress_token,
            progress,
            total,
            message: message.map(Into::into),
        }
    }

    pub fn resource_updated(uri: impl Into<String>) -> Self {
        Self::ResourceUpdated { uri: uri.into() }
    }

    pub fn tools_list_changed() -> Self {
        Self::ToolsListChanged
    }

    pub fn resources_list_changed() -> Self {
        Self::ResourcesListChanged
    }

    pub fn prompts_list_changed() -> Self {
        Self::PromptsListChanged
    }

    pub fn log_message(
        level: impl Into<String>,
        logger: Option<impl Into<String>>,
        data: Value,
    ) -> Self {
        Self::LogMessage {
            level: level.into(),
            logger: logger.map(Into::into),
            data,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct McpClientExchange {
    pub(crate) response: Value,
    pub(crate) notifications: Vec<McpClientNotification>,
}

pub(crate) fn client_notification_from_json_rpc(
    notification: JsonRpcNotification,
) -> McpClientNotification {
    match notification.method() {
        "notifications/progress" => {
            let params = notification.params();
            McpClientNotification::progress(
                params.get("progressToken").cloned().unwrap_or(Value::Null),
                params
                    .get("progress")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                params.get("total").and_then(Value::as_f64),
                params.get("message").and_then(Value::as_str),
            )
        }
        "notifications/resources/updated" => {
            if let Some(uri) = notification.params().get("uri").and_then(Value::as_str) {
                McpClientNotification::resource_updated(uri)
            } else {
                McpClientNotification::Other {
                    method: notification.method().to_string(),
                    params: notification.params().clone(),
                }
            }
        }
        "notifications/tools/list_changed" => McpClientNotification::tools_list_changed(),
        "notifications/resources/list_changed" => McpClientNotification::resources_list_changed(),
        "notifications/prompts/list_changed" => McpClientNotification::prompts_list_changed(),
        "notifications/message" => {
            let params = notification.params();
            if let Some(level) = params.get("level").and_then(Value::as_str) {
                McpClientNotification::log_message(
                    level,
                    params.get("logger").and_then(Value::as_str),
                    params.get("data").cloned().unwrap_or(Value::Null),
                )
            } else {
                McpClientNotification::Other {
                    method: notification.method().to_string(),
                    params: notification.params().clone(),
                }
            }
        }
        method => McpClientNotification::Other {
            method: method.to_string(),
            params: notification.params().clone(),
        },
    }
}

pub(crate) fn mcp_notification_progress_parts(
    notification: &McpClientNotification,
) -> (String, Option<String>) {
    match notification {
        McpClientNotification::Progress { message, .. } => {
            ("mcp_progress".to_string(), message.clone())
        }
        McpClientNotification::ResourceUpdated { uri } => {
            ("mcp_resource_updated".to_string(), Some(uri.clone()))
        }
        McpClientNotification::ToolsListChanged => ("mcp_tools_list_changed".to_string(), None),
        McpClientNotification::ResourcesListChanged => {
            ("mcp_resources_list_changed".to_string(), None)
        }
        McpClientNotification::PromptsListChanged => ("mcp_prompts_list_changed".to_string(), None),
        McpClientNotification::LogMessage { level, .. } => {
            ("mcp_log_message".to_string(), Some(level.clone()))
        }
        McpClientNotification::Other { method, .. } => {
            ("mcp_notification".to_string(), Some(method.clone()))
        }
    }
}
