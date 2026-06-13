use super::fixtures::*;

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
