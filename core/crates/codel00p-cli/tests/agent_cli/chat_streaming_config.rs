use super::support::*;

#[test]
fn agent_run_missing_credential_lists_supported_environment_variables() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p_without_provider_env(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
        ],
    );

    assert!(!output.status.success());
    let error = stderr(&output);
    assert!(error.contains("missing credential for provider `custom`"));
    assert!(error.contains("CODEL00P_PROVIDER_CUSTOM_API_KEY"));
}

#[test]
fn agent_run_rejects_unknown_providers_before_credential_lookup() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p_without_provider_env(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "not-a-provider",
            "--model",
            "gpt-5",
        ],
    );

    assert!(!output.status.success());
    let error = stderr(&output);
    assert!(error.contains("unknown provider: not-a-provider"));
}

#[test]
fn agent_run_rejects_unknown_provider_policy_presets() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
            "--provider-policy-preset",
            "not-a-preset",
        ],
    );

    assert!(!output.status.success());
    let error = stderr(&output);
    assert!(error.contains("unknown provider policy preset: not-a-preset"));
    assert!(error.contains("enterprise_custom_gateway"));
}

#[test]
fn agent_run_applies_provider_policy_preset_before_inference() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Inspect.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            "http://127.0.0.1:9",
            "--provider-policy-preset",
            "enterprise_direct",
        ],
    );

    assert!(!output.status.success());
    let error = stderr(&output);
    assert!(error.contains("provider `custom` was denied by policy: provider is not allowed"));
}

#[test]
fn agent_chat_runs_multiple_turns_and_prints_each_reply() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": { "role": "assistant", "content": "Chat reply." },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
        "first question\nsecond question\n/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(occurrences(&stdout(&output), "Chat reply."), 2);
    assert_eq!(provider.calls(), 2);
}

#[test]
fn agent_chat_threads_conversation_history_to_the_provider() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    // Registered first and matched only when the prior assistant reply is
    // present in the request body, proving the second turn carried history.
    let threaded = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Reply from turn one.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": { "role": "assistant", "content": "Reply from turn two." },
                    "finish_reason": "stop"
                }
            ]
        }));
    });
    let first = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": { "role": "assistant", "content": "Reply from turn one." },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
        "first question\nsecond question\n/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Reply from turn one."), "stdout: {out}");
    assert!(out.contains("Reply from turn two."), "stdout: {out}");
    assert_eq!(threaded.calls(), 1);
    assert_eq!(first.calls(), 1);
}

#[test]
fn agent_chat_exit_command_does_not_call_the_provider() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": { "role": "assistant", "content": "should not happen" },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
        "/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(provider.calls(), 0);
}

#[test]
fn agent_chat_starts_a_fresh_session_each_launch() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let session_id = |output: &Output| {
        stderr(output)
            .lines()
            .find_map(|line| {
                line.split("(session ")
                    .nth(1)
                    .map(|rest| rest.trim_end_matches(')').to_string())
            })
            .expect("session id in banner")
    };

    // Two bare chat launches (no --session-id), each ended immediately by /exit.
    let args = &[
        "agent",
        "chat",
        "--provider",
        "custom",
        "--model",
        "test-model",
    ][..];
    let first = run_codel00p_with_stdin(&db_path, args, "/exit\n");
    let second = run_codel00p_with_stdin(&db_path, args, "/exit\n");

    // Each launch is its own fresh conversation — never the implicit shared
    // default that would replay an unbounded history into every turn.
    let first_id = session_id(&first);
    assert!(first_id.starts_with("chat-"), "fresh chat id: {first_id}");
    assert_ne!(
        first_id,
        session_id(&second),
        "each launch must start a new session"
    );
    assert!(!stderr(&first).contains("Resumed conversation"));
    assert!(!stderr(&second).contains("Resumed conversation"));
}

#[test]
fn agent_chat_rejects_unknown_options() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let output = run_codel00p_with_stdin(&db_path, &["agent", "chat", "--not-a-flag"], "/exit\n");

    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown agent chat option: --not-a-flag"));
}

#[test]
fn agent_chat_resumes_persisted_conversation_history() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    seed_chat_session(
        &db_path,
        "chat-resume",
        &[
            SessionMessage::user("remember the codeword is orchid"),
            SessionMessage::assistant("Understood, the codeword is orchid."),
        ],
    );

    let server = MockServer::start();
    // Only matches once the prior conversation is replayed into the request.
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("remember the codeword is orchid")
            .body_includes("the codeword is orchid");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": { "role": "assistant", "content": "It is orchid." },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "chat-resume",
        ],
        "what was the codeword?\n/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("Resumed conversation with 2 prior message(s)."),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(provider.calls(), 1);
    assert!(stdout(&output).contains("It is orchid."));
}

#[test]
fn agent_chat_tools_and_model_commands_report_state() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "unused" }, "finish_reason": "stop" }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
        "/tools\n/model\n/model swapped-model\n/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("read_file"), "stderr: {err}");
    assert!(err.contains("model test-model"), "stderr: {err}");
    assert!(err.contains("Model set to swapped-model."), "stderr: {err}");
    assert_eq!(provider.calls(), 0);
}

#[test]
fn agent_chat_history_sessions_and_memory_commands() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "Hello there." }, "finish_reason": "stop" }
            ]
        }));
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "chat-tools",
        ],
        "say hi\n/history\n/sessions\n/memory\n/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("user: say hi"), "stderr: {err}");
    assert!(err.contains("assistant: Hello there."), "stderr: {err}");
    assert!(err.contains("chat-tools"), "stderr: {err}");
    assert!(
        err.contains("No approved project memory yet."),
        "stderr: {err}"
    );
}

const STREAM_SSE_BODY: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"content\":\"Streamed \"},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"delta\":{\"content\":\"reply.\"},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

#[test]
fn agent_run_streams_tokens_to_stdout() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""stream":true"#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(STREAM_SSE_BODY);
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Stream this.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--stream",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    provider.assert();
    assert_eq!(stdout(&output), "Streamed reply.\n");
}

#[test]
fn agent_chat_streams_tokens_to_stdout() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes(r#""stream":true"#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(STREAM_SSE_BODY);
    });

    let output = run_codel00p_with_stdin(
        &db_path,
        &[
            "agent",
            "chat",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--stream",
        ],
        "stream this\n/exit\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(provider.calls(), 1);
    assert_eq!(stdout(&output), "Streamed reply.\n");
}
