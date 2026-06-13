use super::support::*;

#[test]
fn agent_run_can_stream_json_events_before_final_text() {
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
                    "message": {
                        "role": "assistant",
                        "content": "Streaming complete."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let output = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Stream events.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--stream-events",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.len() >= 2, "stdout: {stdout}");
    assert!(lines[0].contains(r#""kind":"turn_started""#));
    assert!(
        lines
            .iter()
            .any(|line| line.contains(r#""kind":"turn_completed""#))
    );
    assert_eq!(lines.last(), Some(&"Streaming complete."));
    provider.assert();
}

#[test]
fn agent_run_persists_session_messages_and_events() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Session persistence works."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Persist this turn.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "session-cli",
        ],
    );
    let show = run_codel00p(&db_path, &["session", "show", "session-cli"]);

    assert!(run.status.success(), "stderr: {}", stderr(&run));
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    assert_eq!(stdout(&run), "Session persistence works.\n");
    let session_output = stdout(&show);
    assert!(session_output.contains("1\tmessage\tuser\tPersist this turn.\n"));
    assert!(session_output.contains("2\tmessage\tassistant\tSession persistence works.\n"));
    assert!(session_output.contains("\tevent\tturn_started\t\n"));
    assert!(session_output.contains("\tevent\tturn_completed\t\n"));
    provider.assert();
}

#[test]
fn agent_resume_replays_prior_session_messages_and_appends_only_new_records() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let first_server = MockServer::start();
    let first = first_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Initial request.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Initial answer."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let first_run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Initial request.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &first_server.base_url(),
            "--session-id",
            "session-resume",
        ],
    );
    assert!(first_run.status.success(), "stderr: {}", stderr(&first_run));
    first.assert();

    let second_server = MockServer::start();
    let second = second_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Initial request.")
            .body_includes("Initial answer.")
            .body_includes("Continue with the next step.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Resumed answer."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let resumed = run_codel00p(
        &db_path,
        &[
            "agent",
            "resume",
            "session-resume",
            "Continue with the next step.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &second_server.base_url(),
        ],
    );
    assert!(resumed.status.success(), "stderr: {}", stderr(&resumed));
    assert_eq!(stdout(&resumed), "Resumed answer.\n");
    second.assert();

    let show = run_codel00p(&db_path, &["session", "show", "session-resume"]);
    assert!(show.status.success(), "stderr: {}", stderr(&show));
    let session_output = stdout(&show);
    assert_eq!(
        occurrences(&session_output, "\tmessage\tuser\tInitial request.\n"),
        1
    );
    assert_eq!(
        occurrences(&session_output, "\tmessage\tassistant\tInitial answer.\n"),
        1
    );
    assert_eq!(
        occurrences(
            &session_output,
            "\tmessage\tuser\tContinue with the next step.\n"
        ),
        1
    );
    assert_eq!(
        occurrences(&session_output, "\tmessage\tassistant\tResumed answer.\n"),
        1
    );
}

#[test]
fn agent_run_extracts_reviewed_memory_and_reuses_approved_memory() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Capture memory.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "remember workflow[verify]: Run pnpm verify before pushing main."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let first_run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Capture memory.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "session-memory",
        ],
    );
    assert!(first_run.status.success(), "stderr: {}", stderr(&first_run));
    first.assert();

    let listed = run_codel00p(&db_path, &["memory", "list", "--status", "candidate"]);
    assert!(listed.status.success(), "stderr: {}", stderr(&listed));
    let listed_stdout = stdout(&listed);
    assert!(
        listed_stdout.contains("\tcandidate\tworkflow\tRun pnpm verify before pushing main.\n")
    );
    let memory_id = listed_stdout
        .split('\t')
        .next()
        .expect("memory id")
        .to_string();

    let approve = run_codel00p(
        &db_path,
        &["memory", "approve", &memory_id, "--actor", "alice"],
    );
    assert!(approve.status.success(), "stderr: {}", stderr(&approve));

    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("Use memory.")
            .body_includes("Project memory:")
            .body_includes("kind: workflow")
            .body_includes("Run pnpm verify before pushing main.");
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Loaded reviewed project memory."
                    },
                    "finish_reason": "stop"
                }
            ]
        }));
    });

    let second_run = run_codel00p(
        &db_path,
        &[
            "agent",
            "run",
            "Use memory.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
            "--session-id",
            "session-memory-2",
        ],
    );

    assert!(
        second_run.status.success(),
        "stderr: {}",
        stderr(&second_run)
    );
    assert_eq!(stdout(&second_run), "Loaded reviewed project memory.\n");
    second.assert();
}
