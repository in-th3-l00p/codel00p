use std::{
    path::Path,
    process::{Command, Output},
};

use httpmock::{Method::POST, MockServer};
use serde_json::json;
use tempfile::tempdir;

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home)
        .env("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token")
        .current_dir(home)
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

fn configure_provider(home: &Path, base_url: &str) {
    assert!(
        run(home, &["config", "set", "agent.provider", "custom"])
            .status
            .success()
    );
    assert!(
        run(home, &["config", "set", "agent.model", "test-model"])
            .status
            .success()
    );
    assert!(
        run(home, &["config", "set", "agent.base_url", base_url])
            .status
            .success()
    );
}

#[test]
fn gateway_help_is_a_control_reply() {
    let home = tempdir().expect("tempdir");
    let output = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "/help",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("codel00p agent"));
    // Control commands do not require a provider, so no agent run happened.
}

#[test]
fn gateway_message_runs_a_turn_and_remembers_the_conversation() {
    let home = tempdir().expect("tempdir");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "on it" }, "finish_reason": "stop" }
            ]
        }));
    });
    configure_provider(home.path(), &server.base_url());

    // First message in conversation C1.
    let first = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "check the readme",
        ],
    );
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    assert!(stdout(&first).contains("on it"));

    // Second message in the SAME conversation.
    let second = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "now the changelog",
        ],
    );
    assert!(second.status.success(), "stderr: {}", stderr(&second));

    // Both turns share one durable session (the thread is remembered).
    assert!(provider.calls() >= 2);
    let transcript = stdout(&run(home.path(), &["session", "show", "gateway-c1"]));
    assert!(
        transcript.contains("check the readme"),
        "transcript: {transcript}"
    );
    assert!(
        transcript.contains("now the changelog"),
        "transcript: {transcript}"
    );
}
