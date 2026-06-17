//! Sub-agent delegation primitives.
//!
//! An orchestrator agent hands a focused task to a child agent and waits for its
//! summary. This module defines the role distinction, the task/outcome data, the
//! [`SubAgentSpawner`] seam that actually runs a child, and the `delegate_task`
//! tool an orchestrator calls.
//!
//! This is the in-harness core of the
//! [Sub-Agents initiative](../../../docs/initiatives/subagents-delegation.md),
//! Phase 1. It deliberately stops at the seam: how a child harness is built
//! (model client, narrowed permissions, worktree isolation) lives behind
//! [`SubAgentSpawner`], wired by the application in a later slice.

use std::sync::Arc;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    session::SessionId,
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    workspace::Workspace,
};

/// Whether an agent may delegate to sub-agents.
///
/// A `Leaf` is a focused worker with no `delegate_task` tool, so it cannot spawn
/// children — this is what bounds delegation depth. An `Orchestrator` may spawn
/// workers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRole {
    Leaf,
    Orchestrator,
}

impl AgentRole {
    /// Whether agents in this role may spawn sub-agents.
    pub fn can_delegate(self) -> bool {
        matches!(self, AgentRole::Orchestrator)
    }
}

/// How a delegated child's workspace relates to the parent's.
///
/// `Shared` (the default) runs the child against the parent's workspace, as the
/// classic isolated-context sub-agent does — safe for read-only children.
/// `Worktree` runs the child in its OWN temporary git worktree created off the
/// parent repo's HEAD, so a *mutating* child's file changes never touch the
/// parent working tree; the spawner captures the child's diff and returns it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskIsolation {
    /// Run against the parent workspace (default, backward compatible).
    #[default]
    Shared,
    /// Run in a throwaway git worktree; mutations stay out of the parent tree.
    Worktree,
}

/// A unit of work an orchestrator hands to a child agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegatedTask {
    description: String,
    label: Option<String>,
    isolation: TaskIsolation,
}

impl DelegatedTask {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            label: None,
            isolation: TaskIsolation::Shared,
        }
    }

    /// A short label for progress/audit display (defaults to none).
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Select how the child's workspace is isolated (defaults to
    /// [`TaskIsolation::Shared`]).
    pub fn with_isolation(mut self, isolation: TaskIsolation) -> Self {
        self.isolation = isolation;
        self
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn isolation(&self) -> TaskIsolation {
        self.isolation
    }
}

/// What a finished child agent reports back to its parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationOutcome {
    summary: String,
    child_session_id: SessionId,
    tool_calls: usize,
    /// The child's complete diff, present only for worktree-isolated children
    /// (so the parent can see what the child changed without sharing its tree).
    diff: Option<String>,
}

impl DelegationOutcome {
    pub fn new(summary: impl Into<String>, child_session_id: SessionId, tool_calls: usize) -> Self {
        Self {
            summary: summary.into(),
            child_session_id,
            tool_calls,
            diff: None,
        }
    }

    /// Attach the child's captured diff (worktree isolation).
    pub fn with_diff(mut self, diff: impl Into<String>) -> Self {
        self.diff = Some(diff.into());
        self
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn child_session_id(&self) -> &SessionId {
        &self.child_session_id
    }

    pub fn tool_calls(&self) -> usize {
        self.tool_calls
    }

    pub fn diff(&self) -> Option<&str> {
        self.diff.as_deref()
    }
}

/// Runs a child agent for a delegated task.
///
/// Implementations own how a child harness is built — model client, tool set,
/// and the permission ceiling the child must stay within. The harness depends
/// only on this seam, so delegation can be tested without a real provider.
#[async_trait]
pub trait SubAgentSpawner: Send + Sync {
    async fn spawn(&self, task: DelegatedTask) -> Result<DelegationOutcome, HarnessError>;
}

/// Tool that lets an orchestrator hand a focused task to a child agent and wait
/// for its summary. Registered only for orchestrators (see [`delegation_tools`]).
pub struct DelegateTaskTool {
    spawner: Arc<dyn SubAgentSpawner>,
}

impl DelegateTaskTool {
    pub fn new(spawner: Arc<dyn SubAgentSpawner>) -> Self {
        Self { spawner }
    }
}

#[async_trait]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "Hand a focused task to a child agent and return its summary."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": { "type": "string" },
                "label": { "type": "string" },
                "isolation": {
                    "type": "string",
                    "enum": ["shared", "worktree"],
                    "description": "`worktree` runs a mutating child in its own \
                        throwaway git worktree off HEAD (its diff is returned and \
                        the parent tree is untouched); `shared` (default) runs \
                        against the parent workspace."
                }
            }
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        // Children are isolated runs, so a batch of delegations can execute
        // concurrently; the spawner caps how many run at once. Worktree
        // isolation (`isolation: "worktree"`) gives each mutating child its own
        // throwaway worktree, so even file-writing children are safe to batch —
        // their changes can't collide on a shared working tree.
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        // Spawning a child agent is an external capability gated like a
        // connector; the child's own tool calls are permissioned separately.
        PermissionScope::ExternalConnector
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let description = required_string(self.name(), &input, "task")?;
        let mut task = DelegatedTask::new(description);
        if let Some(label) = optional_string(&input, "label") {
            task = task.with_label(label);
        }
        // Unknown/missing isolation falls back to `Shared` for compatibility.
        let isolation = match optional_string(&input, "isolation") {
            Some("worktree") => TaskIsolation::Worktree,
            _ => TaskIsolation::Shared,
        };
        task = task.with_isolation(isolation);

        let outcome = self.spawner.spawn(task).await?;

        let mut payload = json!({
            "summary": outcome.summary(),
            "child_session_id": outcome.child_session_id().clone(),
            "tool_calls": outcome.tool_calls(),
        });
        if let Some(diff) = outcome.diff() {
            payload["diff"] = json!(diff);
        }
        Ok(ToolResult::json(payload))
    }
}

/// Build the delegation tools available to an agent in `role`.
///
/// A leaf gets an empty registry — no `delegate_task`, so it cannot spawn
/// children, which is what bounds delegation depth. An orchestrator gets the
/// delegate tool backed by `spawner`.
pub fn delegation_tools(role: AgentRole, spawner: Arc<dyn SubAgentSpawner>) -> ToolRegistry {
    match role {
        AgentRole::Leaf => ToolRegistry::new(),
        AgentRole::Orchestrator => {
            ToolRegistry::new().with_tool_arc(Arc::new(DelegateTaskTool::new(spawner)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records the task it was handed and returns a fixed outcome.
    struct StubSpawner {
        seen: Mutex<Vec<DelegatedTask>>,
    }

    impl StubSpawner {
        fn new() -> Self {
            Self {
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl SubAgentSpawner for StubSpawner {
        async fn spawn(&self, task: DelegatedTask) -> Result<DelegationOutcome, HarnessError> {
            self.seen.lock().expect("lock").push(task);
            Ok(DelegationOutcome::new(
                "child done",
                SessionId::from_static("child-1"),
                3,
            ))
        }
    }

    #[test]
    fn only_orchestrators_get_the_delegate_tool() {
        let leaf = delegation_tools(AgentRole::Leaf, Arc::new(StubSpawner::new()));
        assert!(leaf.names().is_empty());
        assert!(!AgentRole::Leaf.can_delegate());

        let orchestrator = delegation_tools(AgentRole::Orchestrator, Arc::new(StubSpawner::new()));
        assert_eq!(orchestrator.names(), vec!["delegate_task".to_string()]);
        assert!(AgentRole::Orchestrator.can_delegate());

        // Delegations are batchable; the spawner caps actual concurrency.
        assert!(orchestrator.is_concurrency_safe("delegate_task", &json!({})));
    }

    #[tokio::test]
    async fn delegate_tool_invokes_spawner_and_returns_summary() {
        let spawner = Arc::new(StubSpawner::new());
        let registry = delegation_tools(AgentRole::Orchestrator, spawner.clone());
        let workspace = Workspace::new(std::env::temp_dir()).expect("workspace");

        let result = registry
            .execute(
                "delegate_task",
                &workspace,
                json!({ "task": "summarize the README", "label": "summary" }),
            )
            .await
            .expect("delegate");

        // The spawner saw the task the orchestrator described.
        let seen = spawner.seen.lock().expect("lock");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].description(), "summarize the README");
        assert_eq!(seen[0].label(), Some("summary"));

        // The child's summary flows back to the parent.
        assert_eq!(
            result.content(),
            &json!({
                "summary": "child done",
                "child_session_id": "child-1",
                "tool_calls": 3,
            })
        );
    }

    #[tokio::test]
    async fn delegate_tool_requires_a_task() {
        let registry = delegation_tools(AgentRole::Orchestrator, Arc::new(StubSpawner::new()));
        let workspace = Workspace::new(std::env::temp_dir()).expect("workspace");

        let error = registry
            .execute("delegate_task", &workspace, json!({ "label": "x" }))
            .await
            .expect_err("missing task should error");
        assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
    }
}
