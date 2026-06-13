use super::fixtures::*;

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
