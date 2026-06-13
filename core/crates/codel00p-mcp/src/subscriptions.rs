//! Long-lived MCP notification workers and stdio reconnection supervision.

use std::{sync::Arc, time::Duration};

use codel00p_protocol::{AgentEvent, EventId, SessionId, TurnId};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
};

use crate::{
    McpClientNotification, McpError, McpStdioClient, StdioServerCommand,
    notifications::mcp_notification_progress_parts,
};

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
    Connected {
        server_id: String,
        attempt: u32,
    },
    Subscribed {
        uri: String,
    },
    Notification {
        notification: McpClientNotification,
    },
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
