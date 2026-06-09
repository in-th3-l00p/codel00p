use async_trait::async_trait;

use crate::events::HarnessEvent;

#[async_trait]
pub trait AgentEventSink: Send + Sync {
    async fn emit(&self, event: &HarnessEvent);
}
