mod support;

use std::{
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, ContextWindowState, HarnessError, HarnessEvent, HarnessInferenceResponse,
    LifecycleHook, ModelToolCall, PermissionDecision, PermissionMode, PermissionPolicy,
    PermissionRequest, PermissionScope, SessionId, SessionMessage, Tool, ToolRegistry, ToolResult,
    TurnLifecycleContext, UserMessage, Workspace,
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
async fn write_tool_calls_request_workspace_write_permission_and_mutate_when_allowed() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-create",
                "create_file",
                json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 42 }\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Created the file."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-write-tool"),
            UserMessage::new("Create src/lib.rs."),
        )
        .await
        .expect("run turn");

    assert_eq!(
        fs::read_to_string(dir.path().join("src/lib.rs")).expect("read created file"),
        "pub fn answer() -> u32 { 42 }\n"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::WorkspaceWrite,
                ..
            } if tool_name == "create_file"
        )
    }));
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::ToolCallCompleted { tool_name, .. } if tool_name == "create_file")
    }));
}

#[tokio::test]
async fn denied_write_tool_calls_do_not_mutate_workspace() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-create-denied",
                "create_file",
                json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 42 }\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Write was denied."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .permission_policy(DenyToolPolicy)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-write-denied"),
            UserMessage::new("Create src/lib.rs."),
        )
        .await
        .expect("run turn");

    assert!(!dir.path().join("src/lib.rs").exists());
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "create_file")
    }));
}

#[tokio::test]
async fn command_tool_calls_request_shell_permission_and_capture_output() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-command",
                "run_command",
                json!({
                    "program": "sh",
                    "args": ["-c", "printf command-output"],
                    "timeout_ms": 1000
                }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Command finished."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::command_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-command-tool"),
            UserMessage::new("Run a command."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::Shell,
                ..
            } if tool_name == "run_command"
        )
    }));
    assert_eq!(
        outcome.tool_calls[0].result.content()["stdout"],
        "command-output"
    );
    assert_eq!(outcome.tool_calls[0].result.content()["success"], true);
}

#[tokio::test]
async fn denied_command_tool_calls_do_not_execute() {
    let dir = tempdir().expect("tempdir");
    let marker = dir.path().join("marker.txt");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-command-denied",
                "run_command",
                json!({
                    "program": "sh",
                    "args": ["-c", "printf denied > marker.txt"],
                    "timeout_ms": 1000
                }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Command denied."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::command_defaults())
        .permission_policy(DenyToolPolicy)
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-command-denied"),
            UserMessage::new("Run a denied command."),
        )
        .await
        .expect("run turn");

    assert!(!marker.exists());
    assert_eq!(
        outcome.tool_calls[0].result.content()["error_kind"],
        "permission_denied"
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(event, HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "run_command")
    }));
}

#[tokio::test]
async fn git_status_requests_read_only_permission() {
    let dir = tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new("call-status", "git_status", json!({}))],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Status checked."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::git_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-git-status"),
            UserMessage::new("Check git status."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::ReadOnly,
                ..
            } if tool_name == "git_status"
        )
    }));
}

#[tokio::test]
async fn git_commit_requests_workspace_write_permission() {
    let dir = tempdir().expect("tempdir");
    init_git_repo(dir.path());
    fs::write(dir.path().join("tracked.txt"), "changed\n").expect("modify tracked");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-commit",
                "git_commit",
                json!({ "message": "change tracked file" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Committed."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::git_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-git-commit"),
            UserMessage::new("Commit the change."),
        )
        .await
        .expect("run turn");

    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::PermissionRequested {
                tool_name,
                scope: PermissionScope::WorkspaceWrite,
                ..
            } if tool_name == "git_commit"
        )
    }));
    assert_eq!(
        outcome.tool_calls[0].result.content()["subject"],
        "change tracked file"
    );
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
async fn blocking_context_window_compacts_session_before_inference() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Compacted context used.",
    )]);
    let mut session_state =
        codel00p_harness::SessionState::new(SessionId::from_static("session-auto-compact"));
    session_state.push_user(UserMessage::new("Request 1."));
    session_state.push_assistant("Answer 1.");
    session_state.push_user(UserMessage::new("Request 2."));
    session_state.push_assistant("Answer 2.");
    session_state.push_user(UserMessage::new("Request 3."));
    session_state.push_assistant("Answer 3.");

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .context_window(
            ContextWindowState::new("gpt-4o", 128_000, 120_000).with_blocking_threshold(100_000),
        )
        .lifecycle_hook(RecordingLifecycleHook {
            calls: calls.clone(),
            fail_on_pre_inference: false,
        })
        .build()
        .expect("build harness")
        .run_turn_with_state(session_state, UserMessage::new("Current request."))
        .await
        .expect("run turn");

    let requests = model.requests();
    let request_messages = requests[0].session_state().messages();
    assert_eq!(request_messages.len(), 5);
    assert_eq!(
        request_messages[0].role(),
        codel00p_protocol::SessionRole::System
    );
    assert!(request_messages[0].content().contains("Session summary:"));
    assert!(request_messages[0].content().contains("Request 1."));
    assert!(request_messages[0].content().contains("Answer 1."));
    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        &[
            "turn_started",
            "pre_compact",
            "pre_inference",
            "turn_completed"
        ]
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::ContextCompacted {
                before_message_count: 7,
                after_message_count: 5,
                ..
            }
        )
    }));
}

#[tokio::test]
async fn lifecycle_hooks_run_in_order_and_fail_nonfatally() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .lifecycle_hook(RecordingLifecycleHook {
            calls: calls.clone(),
            fail_on_pre_inference: true,
        })
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-hooks"),
            UserMessage::new("Run hooks."),
        )
        .await
        .expect("run turn");

    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        &["turn_started", "pre_inference", "turn_completed"]
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::LifecycleHookFailed { hook, message, .. }
                if hook == "pre_inference" && message.contains("pre inference failed")
        )
    }));
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

struct CountingTool {
    executions: Arc<AtomicUsize>,
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

struct RecordingLifecycleHook {
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail_on_pre_inference: bool,
}

fn init_git_repo(path: &std::path::Path) {
    run_git(path, &["init"]);
    run_git(path, &["config", "user.name", "codel00p test"]);
    run_git(path, &["config", "user.email", "codel00p@example.test"]);
    fs::write(path.join("tracked.txt"), "initial\n").expect("write tracked");
    run_git(path, &["add", "tracked.txt"]);
    run_git(path, &["commit", "-m", "initial commit"]);
}

fn run_git(path: &std::path::Path, args: &[&str]) {
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
