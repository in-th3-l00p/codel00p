use std::{
    fs,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codel00p_mcp::{
    McpClientNotification, McpNotificationWorker, McpPromptMessage, McpReconnectPolicy,
    McpStdioClient, McpStdioNotificationSupervisor, McpSubscriptionEvent, McpToolCall,
    StdioServerCommand,
};
use serde_json::json;
use tokio::sync::Mutex;

#[path = "stdio_process_client/basic.rs"]
mod basic;
#[path = "stdio_process_client/lifecycle.rs"]
mod lifecycle;
#[path = "stdio_process_client/notifications.rs"]
mod notifications;
