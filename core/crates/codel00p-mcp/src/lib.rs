use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    io::{BufRead, Write},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use codel00p_harness::{HarnessError, Tool, ToolRegistry, ToolResult, Workspace};
use codel00p_protocol::{AgentEvent, EventId, PermissionScope, SessionId, TurnId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, mpsc},
    task::JoinHandle,
    time::timeout,
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

    #[error("mcp http transport failed for {server_id}: {message}")]
    HttpTransport { server_id: String, message: String },
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

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpClientRoot {
    uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl McpClientRoot {
    pub fn new(uri: impl Into<String>, name: Option<impl Into<String>>) -> Self {
        Self {
            uri: uri.into(),
            name: name.map(Into::into),
        }
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdioServerCommand {
    server_id: String,
    program: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    roots: Vec<McpClientRoot>,
    request_timeout: Duration,
    shutdown_timeout: Duration,
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
            roots: Vec::new(),
            request_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(5),
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

    pub fn with_root(mut self, uri: impl Into<String>, name: Option<impl Into<String>>) -> Self {
        self.roots.push(McpClientRoot::new(uri, name));
        self
    }

    pub fn with_envs<I, K, V>(mut self, env: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.env.extend(
            env.into_iter()
                .map(|(key, value)| (key.as_ref().to_os_string(), value.as_ref().to_os_string())),
        );
        self
    }

    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    pub fn with_shutdown_timeout(mut self, shutdown_timeout: Duration) -> Self {
        self.shutdown_timeout = shutdown_timeout;
        self
    }
}

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
struct McpClientExchange {
    response: Value,
    notifications: Vec<McpClientNotification>,
}

pub struct McpNotificationWorker {
    receiver: mpsc::Receiver<Result<McpClientNotification, McpError>>,
    task: JoinHandle<()>,
}

impl McpNotificationWorker {
    pub fn spawn_stdio<I>(client: Arc<Mutex<McpStdioClient>>, subscriptions: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let subscriptions = subscriptions.into_iter().collect::<Vec<_>>();
        let (sender, receiver) = mpsc::channel(32);
        let task = tokio::spawn(async move {
            for uri in subscriptions {
                let result = client.lock().await.subscribe_resource(uri).await;
                if let Err(error) = result {
                    let _ = sender.send(Err(error)).await;
                    return;
                }
            }

            loop {
                let result = client.lock().await.read_notification().await;
                let should_continue = result.is_ok();
                if sender.send(result).await.is_err() || !should_continue {
                    return;
                }
            }
        });
        Self { receiver, task }
    }

    pub async fn recv(&mut self) -> Option<Result<McpClientNotification, McpError>> {
        self.receiver.recv().await
    }
}

impl Drop for McpNotificationWorker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpReconnectPolicy {
    max_attempts: u32,
    backoff: Duration,
}

impl McpReconnectPolicy {
    pub fn new(max_attempts: u32, backoff: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            backoff,
        }
    }

    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    pub fn backoff(&self) -> Duration {
        self.backoff
    }
}

impl Default for McpReconnectPolicy {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(250))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpSubscriptionEvent {
    Connected { server_id: String, attempt: u32 },
    Subscribed { uri: String },
    Notification { notification: McpClientNotification },
    Reconnecting {
        server_id: String,
        attempt: u32,
        error: String,
    },
}

impl McpSubscriptionEvent {
    pub fn connected(server_id: impl Into<String>, attempt: u32) -> Self {
        Self::Connected {
            server_id: server_id.into(),
            attempt,
        }
    }

    pub fn subscribed(uri: impl Into<String>) -> Self {
        Self::Subscribed { uri: uri.into() }
    }

    pub fn notification(notification: McpClientNotification) -> Self {
        Self::Notification { notification }
    }

    pub fn reconnecting(
        server_id: impl Into<String>,
        attempt: u32,
        error: impl Into<String>,
    ) -> Self {
        Self::Reconnecting {
            server_id: server_id.into(),
            attempt,
            error: error.into(),
        }
    }

    pub fn to_harness_event(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        tool_name: impl Into<String>,
    ) -> AgentEvent {
        let (phase, message) = self.progress_parts();
        AgentEvent::ToolProgress {
            event_id: EventId::new(),
            session_id,
            turn_id,
            tool_name: tool_name.into(),
            phase,
            message,
        }
    }

    fn progress_parts(&self) -> (String, Option<String>) {
        match self {
            Self::Connected { server_id, attempt } => (
                "mcp_subscription_connected".to_string(),
                Some(format!("{server_id} connected on attempt {attempt}")),
            ),
            Self::Subscribed { uri } => {
                ("mcp_subscription_subscribed".to_string(), Some(uri.clone()))
            }
            Self::Notification { notification } => mcp_notification_progress_parts(notification),
            Self::Reconnecting {
                server_id,
                attempt,
                error,
            } => (
                "mcp_subscription_reconnecting".to_string(),
                Some(format!("{server_id} reconnect attempt {attempt}: {error}")),
            ),
        }
    }
}

pub struct McpStdioNotificationSupervisor {
    receiver: mpsc::Receiver<Result<McpSubscriptionEvent, McpError>>,
    task: JoinHandle<()>,
}

impl McpStdioNotificationSupervisor {
    pub fn spawn<I>(
        command: StdioServerCommand,
        subscriptions: I,
        policy: McpReconnectPolicy,
    ) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let subscriptions = subscriptions.into_iter().collect::<Vec<_>>();
        let (sender, receiver) = mpsc::channel(64);
        let task = tokio::spawn(async move {
            run_stdio_notification_supervisor(command, subscriptions, policy, sender).await;
        });
        Self { receiver, task }
    }

    pub async fn recv(&mut self) -> Option<Result<McpSubscriptionEvent, McpError>> {
        self.receiver.recv().await
    }
}

impl Drop for McpStdioNotificationSupervisor {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run_stdio_notification_supervisor(
    command: StdioServerCommand,
    subscriptions: Vec<String>,
    policy: McpReconnectPolicy,
    sender: mpsc::Sender<Result<McpSubscriptionEvent, McpError>>,
) {
    let mut attempt = 1;
    loop {
        let result =
            run_stdio_notification_attempt(command.clone(), &subscriptions, attempt, &sender).await;
        match result {
            Ok(()) => return,
            Err(error) if attempt >= policy.max_attempts() => {
                let _ = sender.send(Err(error)).await;
                return;
            }
            Err(error) => {
                let message = error.to_string();
                attempt += 1;
                if sender
                    .send(Ok(McpSubscriptionEvent::reconnecting(
                        command.server_id().to_string(),
                        attempt,
                        message,
                    )))
                    .await
                    .is_err()
                {
                    return;
                }
                tokio::time::sleep(policy.backoff()).await;
            }
        }
    }
}

async fn run_stdio_notification_attempt(
    command: StdioServerCommand,
    subscriptions: &[String],
    attempt: u32,
    sender: &mpsc::Sender<Result<McpSubscriptionEvent, McpError>>,
) -> Result<(), McpError> {
    let server_id = command.server_id().to_string();
    let mut client = McpStdioClient::spawn(command).await?;
    client.initialize().await?;
    send_subscription_event(sender, McpSubscriptionEvent::connected(server_id, attempt)).await?;

    for uri in subscriptions {
        client.subscribe_resource(uri.clone()).await?;
        send_subscription_event(sender, McpSubscriptionEvent::subscribed(uri.clone())).await?;
    }

    loop {
        let notification = client.read_notification().await?;
        send_subscription_event(sender, McpSubscriptionEvent::notification(notification)).await?;
    }
}

async fn send_subscription_event(
    sender: &mpsc::Sender<Result<McpSubscriptionEvent, McpError>>,
    event: McpSubscriptionEvent,
) -> Result<(), McpError> {
    sender.send(Ok(event)).await.map_err(|_| McpError::Client {
        message: "mcp subscription receiver dropped".to_string(),
    })
}

fn client_capabilities(roots: &[McpClientRoot]) -> Value {
    let mut capabilities = serde_json::Map::new();
    if !roots.is_empty() {
        capabilities.insert(
            "roots".to_string(),
            serde_json::json!({
                "listChanged": false
            }),
        );
    }
    Value::Object(capabilities)
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
        let response = self
            .request("tools/list", Value::Object(Default::default()))
            .await?;
        parse_tool_descriptors(&self.server_id, response, |message| {
            McpError::HttpTransport {
                server_id: self.server_id.clone(),
                message,
            }
        })
    }

    pub async fn list_resources(&mut self) -> Result<Vec<McpResourceDescriptor>, McpError> {
        let response = self
            .request("resources/list", Value::Object(Default::default()))
            .await?;
        parse_resource_descriptors(&self.server_id, response, |message| {
            McpError::HttpTransport {
                server_id: self.server_id.clone(),
                message,
            }
        })
    }

    pub async fn list_prompts(&mut self) -> Result<Vec<McpPromptDescriptor>, McpError> {
        let response = self
            .request("prompts/list", Value::Object(Default::default()))
            .await?;
        parse_prompt_descriptors(&self.server_id, response, |message| {
            McpError::HttpTransport {
                server_id: self.server_id.clone(),
                message,
            }
        })
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

fn json_rpc_result_from_response<F>(
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

fn parse_initialization_response<F>(
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

fn parse_tool_descriptors<F>(
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

fn parse_resource_descriptors<F>(
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

fn parse_prompt_descriptors<F>(
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
                            let name =
                                argument.get("name").and_then(Value::as_str).ok_or_else(|| {
                                    error("prompt argument omitted name".to_string())
                                })?;
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

fn parse_prompt_output<F>(response: Value, error: F) -> Result<McpPromptOutput, McpError>
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

fn decode_sse_json_rpc_messages(body: &str) -> Result<Vec<Value>, String> {
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

fn client_exchange_from_messages<F>(
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

fn client_notification_from_json_rpc(notification: JsonRpcNotification) -> McpClientNotification {
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

    async fn respond_to_server_request(
        &mut self,
        request: JsonRpcRequest,
    ) -> Result<(), McpError> {
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

    pub async fn list_prompts(&mut self) -> Result<Vec<McpPromptDescriptor>, McpError> {
        let response = self
            .request("prompts/list", Value::Object(Default::default()))
            .await?;
        parse_prompt_descriptors(&self.server_id, response, |message| self.stdio_error(message))
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

fn mcp_notification_progress_parts(notification: &McpClientNotification) -> (String, Option<String>) {
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

pub fn crate_name() -> &'static str {
    "codel00p-mcp"
}
