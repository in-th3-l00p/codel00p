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
    delegation::{DelegatedTask, DelegationOutcome, SubAgentSpawner, TaskIsolation},
    errors::HarnessError,
    git::{DEFAULT_DIFF_BYTES, cap_string, git_output},
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
        self.build_child_in(self.workspace.clone())
    }

    /// Build the child harness against `workspace`, keeping the SAME tool set,
    /// permission ceiling, and limits. Worktree isolation uses this to point a
    /// child at a throwaway worktree instead of the shared parent workspace.
    fn build_child_in(&self, workspace: Workspace) -> Result<AgentHarness, HarnessError> {
        let mut builder = AgentHarness::builder()
            .model_client_arc(self.model_client.clone())
            .workspace(workspace)
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

    /// Run a child in the shared parent workspace (classic isolated-context
    /// sub-agent). No diff is captured.
    async fn spawn_shared(&self, task: &DelegatedTask) -> Result<DelegationOutcome, HarnessError> {
        let child_session = SessionId::new();
        let child = self.build_child()?;
        let outcome = child
            .run_turn(child_session.clone(), UserMessage::new(task.description()))
            .await?;
        let summary = summary_of(outcome.assistant_message);
        Ok(DelegationOutcome::new(
            summary,
            child_session,
            outcome.tool_calls.len(),
        ))
    }

    /// Run a mutating child in its own temporary git worktree off the parent
    /// repo's HEAD, capture its complete diff, then tear the worktree down. The
    /// parent working tree is never touched.
    async fn spawn_worktree(
        &self,
        task: &DelegatedTask,
    ) -> Result<DelegationOutcome, HarnessError> {
        // (a) Worktree isolation only makes sense inside a git repo.
        git_output(
            "delegate_task",
            &self.workspace,
            &["rev-parse", "--git-dir"],
        )
        .map_err(|_| HarnessError::ToolFailed {
            name: "delegate_task".to_string(),
            message: "worktree isolation requires the parent workspace to be a git \
                          repository (run `git init` or use isolation \"shared\")"
                .to_string(),
        })?;

        // (b) Create a throwaway worktree OUTSIDE the repo, detached at HEAD.
        let temp = tempfile::tempdir().map_err(|error| HarnessError::ToolFailed {
            name: "delegate_task".to_string(),
            message: format!("failed to create a temp dir for the worktree: {error}"),
        })?;
        let worktree_path = temp.path().join("wt");
        let worktree_arg = worktree_path.to_string_lossy().to_string();
        git_output(
            "delegate_task",
            &self.workspace,
            &["worktree", "add", "--detach", &worktree_arg, "HEAD"],
        )?;

        // From here on we must clean the worktree up regardless of outcome.
        let result = self.run_in_worktree(task, &worktree_path).await;

        // (f) Best-effort cleanup; never let teardown lose the child's result.
        let _ = git_output(
            "delegate_task",
            &self.workspace,
            &["worktree", "remove", "--force", &worktree_arg],
        );
        let _ = git_output("delegate_task", &self.workspace, &["worktree", "prune"]);
        drop(temp);

        result
    }

    /// Build + run the child in `worktree_path` and capture its full diff.
    async fn run_in_worktree(
        &self,
        task: &DelegatedTask,
        worktree_path: &std::path::Path,
    ) -> Result<DelegationOutcome, HarnessError> {
        let child_workspace = Workspace::new(worktree_path)?;
        let child_session = SessionId::new();
        let child = self.build_child_in(child_workspace.clone())?;
        let outcome = child
            .run_turn(child_session.clone(), UserMessage::new(task.description()))
            .await?;
        let summary = summary_of(outcome.assistant_message);

        // (e) Capture the COMPLETE diff, including new/untracked files: stage
        // everything, then diff the index. Cap it like `git_diff` so a huge
        // diff can't blow the parent context.
        git_output("delegate_task", &child_workspace, &["add", "-A"])?;
        let raw = git_output(
            "delegate_task",
            &child_workspace,
            &["diff", "--cached", "--no-ext-diff"],
        )?;
        let diff = cap_string(&raw, DEFAULT_DIFF_BYTES).content;

        Ok(
            DelegationOutcome::new(summary, child_session, outcome.tool_calls.len())
                .with_diff(diff),
        )
    }
}

fn summary_of(assistant_message: Option<String>) -> String {
    assistant_message
        .unwrap_or_else(|| "(the sub-agent finished without a text summary)".to_string())
}

#[async_trait]
impl SubAgentSpawner for HarnessSubAgentSpawner {
    async fn spawn(&self, task: DelegatedTask) -> Result<DelegationOutcome, HarnessError> {
        match task.isolation() {
            TaskIsolation::Shared => self.spawn_shared(&task).await,
            TaskIsolation::Worktree => self.spawn_worktree(&task).await,
        }
    }
}
