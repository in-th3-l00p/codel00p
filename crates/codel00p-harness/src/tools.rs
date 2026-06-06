use async_trait::async_trait;
use serde_json::Value;

use crate::{errors::HarnessError, tool_result::ToolResult, workspace::Workspace};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;

    fn description(&self) -> &'static str;

    fn input_schema(&self) -> Value;

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError>;
}
