//! Shared fixtures for agent tool-loop integration tests.

pub(crate) use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub(crate) use async_trait::async_trait;
pub(crate) use codel00p_harness::{
    AgentHarness, ContextWindowState, HarnessError, HarnessEvent, HarnessInferenceResponse,
    LifecycleHook, ModelToolCall, PermissionDecision, PermissionMode, PermissionPolicy,
    PermissionRequest, PermissionScope, SessionId, SessionMessage, Tool, ToolRegistry, ToolResult,
    TurnLifecycleContext, UserMessage, Workspace,
};
pub(crate) use serde_json::{Value, json};
pub(crate) use tempfile::tempdir;
pub(crate) use tokio::time::Instant;

pub(crate) use super::support::ScriptedModelClient;

pub(crate) struct SlowReadOnlyTool(pub(crate) &'static str);

#[async_trait]
impl Tool for SlowReadOnlyTool {
    fn name(&self) -> &str {
        self.0
    }

    fn description(&self) -> &str {
        "Slow read-only test tool."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(ToolResult::json(json!({ "tool": self.0 })))
    }
}

pub(crate) struct TrackingTool {
    pub(crate) name: &'static str,
    pub(crate) concurrency_safe: bool,
    pub(crate) active: Arc<AtomicUsize>,
    pub(crate) max_active: Arc<AtomicUsize>,
}

impl TrackingTool {
    pub(crate) fn new(
        name: &'static str,
        concurrency_safe: bool,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            name,
            concurrency_safe,
            active,
            max_active,
        }
    }
}

#[async_trait]
impl Tool for TrackingTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "Tracking test tool."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        self.concurrency_safe
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);

        Ok(ToolResult::json(json!({ "tool": self.name })))
    }
}

pub(crate) struct CountingTool {
    pub(crate) executions: Arc<AtomicUsize>,
}

pub(crate) struct ProgressTool;

#[async_trait]
impl Tool for ProgressTool {
    fn name(&self) -> &str {
        "progress_tool"
    }

    fn description(&self) -> &str {
        "Returns tool progress metadata."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        Ok(ToolResult::json(json!({ "ok": true }))
            .with_progress("mcp_progress", Some("Half done"))
            .with_progress("mcp_resource_updated", Some("codel00p://memory/mem-1")))
    }
}

#[async_trait]
impl Tool for CountingTool {
    fn name(&self) -> &str {
        "counting"
    }

    fn description(&self) -> &str {
        "Counts executions."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(ToolResult::json(json!({ "executed": true })))
    }
}

pub(crate) struct DenyToolPolicy;

#[async_trait]
impl PermissionPolicy for DenyToolPolicy {
    async fn decide(&self, request: PermissionRequest) -> Result<PermissionDecision, HarnessError> {
        Ok(PermissionDecision::deny(
            request.id(),
            PermissionMode::Deny,
            format!("{} is blocked in this test", request.tool_name()),
        ))
    }
}

pub(crate) struct RecordingLifecycleHook {
    pub(crate) calls: Arc<Mutex<Vec<&'static str>>>,
    pub(crate) fail_on_pre_inference: bool,
}

pub(crate) fn init_git_repo(path: &std::path::Path) {
    run_git(path, &["init"]);
    run_git(path, &["config", "user.name", "codel00p test"]);
    run_git(path, &["config", "user.email", "codel00p@example.test"]);
    fs::write(path.join("tracked.txt"), "initial\n").expect("write tracked");
    run_git(path, &["add", "tracked.txt"]);
    run_git(path, &["commit", "-m", "initial commit"]);
}

pub(crate) fn run_git(path: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[async_trait]
impl LifecycleHook for RecordingLifecycleHook {
    async fn on_turn_started(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        self.calls.lock().expect("calls").push("turn_started");
        Ok(())
    }

    async fn on_pre_inference(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        self.calls.lock().expect("calls").push("pre_inference");
        if self.fail_on_pre_inference {
            return Err(HarnessError::Configuration {
                message: "pre inference failed".to_string(),
            });
        }
        Ok(())
    }

    async fn on_pre_compact(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        self.calls.lock().expect("calls").push("pre_compact");
        Ok(())
    }

    async fn on_turn_completed(&self, _context: TurnLifecycleContext) -> Result<(), HarnessError> {
        self.calls.lock().expect("calls").push("turn_completed");
        Ok(())
    }
}
