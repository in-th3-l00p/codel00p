use async_trait::async_trait;

use crate::{
    errors::HarnessError,
    session::{SessionId, TurnId},
};

#[derive(Clone, Debug)]
pub struct TurnLifecycleContext {
    session_id: SessionId,
    turn_id: TurnId,
    message_count: usize,
}

impl TurnLifecycleContext {
    pub fn new(session_id: SessionId, turn_id: TurnId, message_count: usize) -> Self {
        Self {
            session_id,
            turn_id,
            message_count,
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }
}

#[async_trait]
pub trait LifecycleHook: Send + Sync {
    async fn on_turn_started(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn on_pre_inference(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn on_post_tool(
        &self,
        _context: TurnLifecycleContext,
        _tool_name: &str,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn on_pre_compact(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn on_turn_completed(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        Ok(())
    }
}
