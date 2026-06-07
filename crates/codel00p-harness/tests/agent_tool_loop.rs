mod support;

use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, ContextWindowState, HarnessError, HarnessEvent, HarnessInferenceResponse,
    ModelToolCall, PermissionDecision, PermissionMode, PermissionPolicy, PermissionRequest,
    SessionId, SessionMessage, Tool, ToolRegistry, ToolResult, UserMessage, Workspace,
};
use serde_json::{Value, json};
use support::ScriptedModelClient;
use tempfile::tempdir;
use tokio::time::Instant;

#[tokio::test]
async fn executes_tool_calls_and_continues_until_final_text() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "README contains Agent Harness."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-tools"),
            UserMessage::new("Read README."),
        )
        .await
        .expect("run turn");

    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("README contains Agent Harness.")
    );
    assert_eq!(model.requests().len(), 2);
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "read_file");
    assert_eq!(
        outcome.session_state.messages(),
        &[
            SessionMessage::user("Read README."),
            SessionMessage::assistant_tool_calls(vec![ModelToolCall::new(
                "call-1",
                "read_file",
                json!({ "path": "README.md" }),
            )]),
            SessionMessage::tool_result(
                "call-1",
                "read_file",
                r#"{"content":"Agent Harness\n","path":"README.md"}"#,
            ),
            SessionMessage::assistant("README contains Agent Harness."),
        ]
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallCompleted { tool_name, .. } if tool_name == "read_file")
    }));
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { iterations: 2, .. })
    ));
}

#[tokio::test]
async fn unknown_tool_call_is_recorded_as_failed_tool_result() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-missing", "missing", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "The tool was unavailable."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-missing"),
            UserMessage::new("Use missing."),
        )
        .await
        .expect("run turn");

    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "missing");
    assert!(
        outcome.tool_calls[0].result.content()["error"]
            .as_str()
            .unwrap()
            .contains("tool not found")
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallFailed { tool_name, .. } if tool_name == "missing")
    }));
}

#[tokio::test]
async fn denied_tool_call_is_recorded_without_executing_tool() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-denied", "counting", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Tool was denied."),
    ]);
    let executions = Arc::new(AtomicUsize::new(0));

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::new().with_tool(CountingTool {
            executions: executions.clone(),
        }))
        .permission_policy(DenyToolPolicy)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-denied"),
            UserMessage::new("Try denied tool."),
        )
        .await
        .expect("run turn");

    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "counting");
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "counting")
    }));
}

#[tokio::test]
async fn context_window_state_is_sent_to_model_client() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);

    AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .context_window(
            ContextWindowState::new("gpt-4o", 128_000, 64_000).with_warning_threshold(100_000),
        )
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-context-window"),
            UserMessage::new("Report context."),
        )
        .await
        .expect("run turn");

    let requests = model.requests();
    assert_eq!(
        requests[0]
            .context_window()
            .expect("context window")
            .model(),
        "gpt-4o"
    );
    assert_eq!(
        requests[0]
            .context_window()
            .expect("context window")
            .used_tokens(),
        64_000
    );
}

#[tokio::test]
async fn iteration_limit_stops_infinite_tool_call_loop() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-2",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
    ]);

    let error = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(1)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-limit"),
            UserMessage::new("Loop."),
        )
        .await
        .expect_err("iteration limit should fail");

    assert!(matches!(error, HarnessError::IterationLimit { limit: 1 }));
}

#[tokio::test]
async fn concurrency_safe_tool_calls_run_in_parallel_batches() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![
                ModelToolCall::new("call-slow-a", "slow_a", json!({})),
                ModelToolCall::new("call-slow-b", "slow_b", json!({})),
            ],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Both tools finished."),
    ]);
    let tools = ToolRegistry::new()
        .with_tool(SlowReadOnlyTool("slow_a"))
        .with_tool(SlowReadOnlyTool("slow_b"));

    let started = Instant::now();
    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(tools)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-concurrent"),
            UserMessage::new("Run slow tools."),
        )
        .await
        .expect("run turn");

    assert!(
        started.elapsed() < Duration::from_millis(350),
        "concurrency-safe tools should run in one batch"
    );
    assert_eq!(outcome.tool_calls.len(), 2);
    assert_eq!(outcome.tool_calls[0].name, "slow_a");
    assert_eq!(outcome.tool_calls[1].name, "slow_b");
}

#[tokio::test]
async fn unsafe_tool_calls_split_parallel_batches() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![
                ModelToolCall::new("call-safe-a", "safe_a", json!({})),
                ModelToolCall::new("call-unsafe", "unsafe", json!({})),
                ModelToolCall::new("call-safe-b", "safe_b", json!({})),
            ],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Serial boundary respected."),
    ]);
    let tools = ToolRegistry::new()
        .with_tool(TrackingTool::new(
            "safe_a",
            true,
            active.clone(),
            max_active.clone(),
        ))
        .with_tool(TrackingTool::new(
            "unsafe",
            false,
            active.clone(),
            max_active.clone(),
        ))
        .with_tool(TrackingTool::new(
            "safe_b",
            true,
            active,
            max_active.clone(),
        ));

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(tools)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-serial-boundary"),
            UserMessage::new("Run mixed tools."),
        )
        .await
        .expect("run turn");

    assert_eq!(outcome.tool_calls.len(), 3);
    assert_eq!(outcome.tool_calls[0].name, "safe_a");
    assert_eq!(outcome.tool_calls[1].name, "unsafe");
    assert_eq!(outcome.tool_calls[2].name, "safe_b");
    assert_eq!(
        max_active.load(Ordering::SeqCst),
        1,
        "unsafe tools should prevent adjacent safe tools from sharing a batch"
    );
}

struct SlowReadOnlyTool(&'static str);

#[async_trait]
impl Tool for SlowReadOnlyTool {
    fn name(&self) -> &'static str {
        self.0
    }

    fn description(&self) -> &'static str {
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

struct TrackingTool {
    name: &'static str,
    concurrency_safe: bool,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

impl TrackingTool {
    fn new(
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
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
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

struct CountingTool {
    executions: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for CountingTool {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn description(&self) -> &'static str {
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

struct DenyToolPolicy;

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
