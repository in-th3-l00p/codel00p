use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use httpmock::{Method::POST, MockServer};
use serde_json::json;
use tempfile::tempdir;

fn run_codel00p(db_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .arg("--memory-db")
        .arg(db_path)
        .arg("--organization-id")
        .arg("org-1")
        .arg("--project-id")
        .arg("project-1")
        .arg("--project-name")
        .arg("codel00p")
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .args(args)
        .output()
        .expect("run codel00p")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

#[test]
fn agent_run_calls_provider_with_read_only_tools_and_prints_final_text() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");
    let workspace = dir.path().join("workspace");
    fs::create_dir(&workspace).expect("create workspace");
    fs::write(workspace.join("README.md"), "# codel00p\n").expect("write readme");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .header("authorization", "Bearer test-token")
            .body_includes(r#""model":"test-model""#)
            .body_includes(r#""role":"user""#)
            .body_includes(r#""content":"Inspect this project.""#)
            .body_includes(r#""name":"read_file""#)
            .body_includes(r#""name":"search_text""#)
            .body_includes(r#""name":"list_files""#);
        then.status(200).json_body(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "The project has a README."
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
            "Inspect this project.",
            "--workspace",
            workspace.to_str().expect("workspace path"),
            "--provider",
            "custom",
            "--model",
            "test-model",
            "--base-url",
            &server.base_url(),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "The project has a README.\n");
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
