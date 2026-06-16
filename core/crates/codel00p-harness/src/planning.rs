//! The `update_plan` tool: a lightweight, structured to-do list the agent
//! maintains while it works.
//!
//! Mature coding agents keep an explicit plan (Claude Code's `TodoWrite`,
//! Codex's `update_plan`) so multi-step work stays on track and the current
//! intent is visible. The model rewrites the whole plan each call; the harness
//! validates it (one in-progress step at a time), stores the latest version in a
//! shared [`PlanStore`] a UI can read, and echoes it back so the model keeps its
//! own working memory.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, optional_string},
    workspace::Workspace,
};

/// Maximum number of steps in a plan, to keep it a plan and not a transcript.
const MAX_STEPS: usize = 50;

/// A single plan step and its progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanItem {
    pub step: String,
    pub status: PlanStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
}

impl PlanStatus {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

/// A shared, cloneable handle to the current plan, so a UI or the harness can
/// read what the agent most recently set.
#[derive(Clone, Default)]
pub struct PlanStore {
    inner: Arc<Mutex<Vec<PlanItem>>>,
}

impl PlanStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the stored plan.
    pub fn set(&self, plan: Vec<PlanItem>) {
        *self.inner.lock().expect("plan lock") = plan;
    }

    /// A snapshot of the current plan.
    pub fn current(&self) -> Vec<PlanItem> {
        self.inner.lock().expect("plan lock").clone()
    }
}

/// Create / update the agent's working plan.
pub struct UpdatePlanTool {
    store: PlanStore,
}

impl UpdatePlanTool {
    pub fn new(store: PlanStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for UpdatePlanTool {
    fn name(&self) -> &str {
        "update_plan"
    }

    fn description(&self) -> &str {
        "Record or update your working plan for a multi-step task. Pass the full \
         `plan` each time as an ordered list of { step, status } where status is \
         `pending`, `in_progress`, or `completed`. Keep exactly one step \
         `in_progress` at a time, and mark steps `completed` as you finish them. \
         Use this to plan non-trivial work and to keep your progress visible; skip \
         it for single-step tasks."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["plan"],
            "properties": {
                "plan": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["step", "status"],
                        "properties": {
                            "step": { "type": "string" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            }
                        }
                    }
                },
                "explanation": { "type": "string" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let items = input
            .get("plan")
            .and_then(Value::as_array)
            .ok_or_else(|| self.invalid("missing array field `plan`"))?;
        if items.is_empty() {
            return Err(self.invalid("`plan` must not be empty"));
        }
        if items.len() > MAX_STEPS {
            return Err(self.invalid(format!(
                "plan has {} steps; the maximum is {MAX_STEPS}",
                items.len()
            )));
        }

        let mut plan = Vec::with_capacity(items.len());
        let mut in_progress = 0usize;
        for (index, item) in items.iter().enumerate() {
            let step = item
                .get("step")
                .and_then(Value::as_str)
                .filter(|step| !step.trim().is_empty())
                .ok_or_else(|| self.invalid(format!("step {index}: missing non-empty `step`")))?;
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .and_then(PlanStatus::parse)
                .ok_or_else(|| {
                    self.invalid(format!(
                        "step {index}: `status` must be one of pending, in_progress, completed"
                    ))
                })?;
            if status == PlanStatus::InProgress {
                in_progress += 1;
            }
            plan.push(PlanItem {
                step: step.to_string(),
                status,
            });
        }

        if in_progress > 1 {
            return Err(self.invalid(format!(
                "{in_progress} steps are in_progress; keep exactly one step in_progress at a time"
            )));
        }

        self.store.set(plan.clone());

        let completed = plan
            .iter()
            .filter(|item| item.status == PlanStatus::Completed)
            .count();
        let rendered: Vec<Value> = plan
            .iter()
            .map(|item| json!({ "step": item.step, "status": item.status.label() }))
            .collect();

        Ok(ToolResult::json(json!({
            "plan": rendered,
            "total": plan.len(),
            "completed": completed,
            "explanation": optional_string(&input, "explanation"),
        })))
    }
}

impl UpdatePlanTool {
    fn invalid(&self, message: impl Into<String>) -> HarnessError {
        HarnessError::InvalidToolInput {
            name: self.name().to_string(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        (dir, ws)
    }

    #[tokio::test]
    async fn stores_and_echoes_a_valid_plan() {
        let (_dir, ws) = workspace();
        let store = PlanStore::new();
        let tool = UpdatePlanTool::new(store.clone());

        let result = tool
            .execute(
                &ws,
                json!({
                    "plan": [
                        { "step": "Read the code", "status": "completed" },
                        { "step": "Write the fix", "status": "in_progress" },
                        { "step": "Run tests", "status": "pending" }
                    ]
                }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["total"], 3);
        assert_eq!(content["completed"], 1);
        assert_eq!(content["plan"][1]["status"], "in_progress");

        // The store reflects the latest plan for any UI to read.
        let stored = store.current();
        assert_eq!(stored.len(), 3);
        assert_eq!(stored[1].status, PlanStatus::InProgress);
    }

    #[tokio::test]
    async fn rejects_more_than_one_in_progress() {
        let (_dir, ws) = workspace();
        let tool = UpdatePlanTool::new(PlanStore::new());
        let error = tool
            .execute(
                &ws,
                json!({
                    "plan": [
                        { "step": "A", "status": "in_progress" },
                        { "step": "B", "status": "in_progress" }
                    ]
                }),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
    }

    #[tokio::test]
    async fn rejects_unknown_status() {
        let (_dir, ws) = workspace();
        let tool = UpdatePlanTool::new(PlanStore::new());
        let error = tool
            .execute(&ws, json!({ "plan": [{ "step": "A", "status": "doing" }] }))
            .await
            .unwrap_err();
        assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
    }

    #[tokio::test]
    async fn latest_call_replaces_the_plan() {
        let (_dir, ws) = workspace();
        let store = PlanStore::new();
        let tool = UpdatePlanTool::new(store.clone());

        tool.execute(
            &ws,
            json!({ "plan": [{ "step": "A", "status": "pending" }] }),
        )
        .await
        .unwrap();
        tool.execute(
            &ws,
            json!({ "plan": [{ "step": "A", "status": "completed" }, { "step": "B", "status": "in_progress" }] }),
        )
        .await
        .unwrap();

        let stored = store.current();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].status, PlanStatus::Completed);
    }
}
