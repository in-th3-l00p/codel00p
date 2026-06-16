//! A default [`SubAgentSpawner`] that runs delegated tasks as real child agents.
//!
//! [`delegation`](crate::delegation) defines the spawner *seam*; this module
//! provides the batteries-included implementation so `delegate_task` works out of
//! the box. Each delegation runs as an **isolated child [`AgentHarness`] turn**:
//! a fresh session, the child's own (typically leaf) tool set, and its own
//! permission ceiling. The parent only ever sees the child's final summary — the
//! classic isolated-context sub-agent (cf. Claude Code's `Agent`/`Task`).

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    agent::AgentHarness,
    delegation::{DelegatedTask, DelegationOutcome, SubAgentSpawner},
    errors::HarnessError,
    permissions::PermissionPolicy,
    session::{SessionId, UserMessage},
    tool_registry::ToolRegistry,
    turn::ModelClient,
    workspace::Workspace,
};

const DEFAULT_CHILD_MAX_ITERATIONS: u32 = 8;

/// Runs each delegated task as an isolated child [`AgentHarness`].
///
/// Clone is cheap (everything is `Arc`/`Clone`); a fresh child harness is built
/// per task because a turn consumes its harness.
#[derive(Clone)]
pub struct HarnessSubAgentSpawner {
    model_client: Arc<dyn ModelClient>,
    workspace: Workspace,
    tools: ToolRegistry,
    permission_policy: Option<Arc<dyn PermissionPolicy>>,
    max_iterations: u32,
    max_tool_result_bytes: Option<usize>,
}

impl HarnessSubAgentSpawner {
    /// Builds a spawner. `tools` is the child's tool set — pass a **leaf** set
    /// (no `delegate_task`) to bound delegation depth. The child shares the
    /// parent's model client and workspace by default.
    pub fn new(
        model_client: Arc<dyn ModelClient>,
        workspace: Workspace,
        tools: ToolRegistry,
    ) -> Self {
        Self {
            model_client,
            workspace,
            tools,
            permission_policy: None,
            max_iterations: DEFAULT_CHILD_MAX_ITERATIONS,
            max_tool_result_bytes: None,
        }
    }

    /// Applies a permission ceiling to the child (defaults to allow-all, as the
    /// harness does).
    pub fn permission_policy(mut self, policy: Arc<dyn PermissionPolicy>) -> Self {
        self.permission_policy = Some(policy);
        self
    }

    /// Caps how many tool-call iterations a child may run.
    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Caps the byte size of each child tool result (see
    /// [`AgentHarnessBuilder::max_tool_result_bytes`](crate::AgentHarnessBuilder)).
    pub fn max_tool_result_bytes(mut self, max_bytes: usize) -> Self {
        self.max_tool_result_bytes = Some(max_bytes);
        self
    }

    fn build_child(&self) -> Result<AgentHarness, HarnessError> {
        let mut builder = AgentHarness::builder()
            .model_client_arc(self.model_client.clone())
            .workspace(self.workspace.clone())
            .tools(self.tools.clone())
            .max_iterations(self.max_iterations);
        if let Some(policy) = &self.permission_policy {
            builder = builder.permission_policy_arc(policy.clone());
        }
        if let Some(bytes) = self.max_tool_result_bytes {
            builder = builder.max_tool_result_bytes(bytes);
        }
        builder.build()
    }
}

#[async_trait]
impl SubAgentSpawner for HarnessSubAgentSpawner {
    async fn spawn(&self, task: DelegatedTask) -> Result<DelegationOutcome, HarnessError> {
        let child_session = SessionId::new();
        let child = self.build_child()?;
        let outcome = child
            .run_turn(child_session.clone(), UserMessage::new(task.description()))
            .await?;
        let summary = outcome
            .assistant_message
            .unwrap_or_else(|| "(the sub-agent finished without a text summary)".to_string());
        Ok(DelegationOutcome::new(
            summary,
            child_session,
            outcome.tool_calls.len(),
        ))
    }
}
