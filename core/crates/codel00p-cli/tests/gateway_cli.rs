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

/// Register a scripted provider for the live approval flow. The four stages are
/// matched deterministically on conversation contents (the established pattern),
/// so mock-definition order is irrelevant:
///  A. initial message (no tool result yet)        -> request `create_file`
///  B. after the permission denial                 -> final "awaiting approval"
///  C. resumed after `/approve`                     -> request `create_file` again
///  D. after the file was created                   -> final "done"
fn script_create_file_flow(server: &MockServer) {
    let tool_call = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-create",
                    "type": "function",
                    "function": {
                        "name": "create_file",
                        "arguments": "{\"path\":\"gw-approved.txt\",\"content\":\"written-by-gateway\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    // A. First model call: nothing has executed yet.
    let call = tool_call.clone();
    server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_excludes(r#""role":"tool""#);
        then.status(200).json_body(call);
    });
    // B. The privileged tool was denied; park and ask the user.
    server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("permission_denied")
            .body_excludes("approved your pending request")
            .body_excludes("bytes_written");
        then.status(200).json_body(json!({
            "choices": [{
                "message": { "role": "assistant", "content": "Waiting for your approval." },
                "finish_reason": "stop"
            }]
        }));
    });
    // C. Resumed after /approve: request the tool again (now one-shot granted).
    server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("approved your pending request")
            .body_excludes("bytes_written");
        then.status(200).json_body(tool_call);
    });
    // D. The file was created; finish.
    server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_includes("bytes_written");
        then.status(200).json_body(json!({
            "choices": [{
                "message": { "role": "assistant", "content": "Created the file." },
                "finish_reason": "stop"
            }]
        }));
    });
}

#[test]
fn gateway_privileged_tool_parks_for_remote_approval() {
    let home = tempdir().expect("tempdir");
    let server = MockServer::start();
    script_create_file_flow(&server);
    configure_provider(home.path(), &server.base_url());

    let parked = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "please create gw-approved.txt",
        ],
    );
    assert!(parked.status.success(), "stderr: {}", stderr(&parked));
    let reply = stdout(&parked);
    // The remote user is told what is wanted and how to allow it.
    assert!(reply.contains("Approval needed"), "reply: {reply}");
    assert!(reply.contains("create_file"), "reply: {reply}");
    assert!(reply.contains("/approve"), "reply: {reply}");
    // The privileged tool did NOT run while parked.
    assert!(
        !home.path().join("gw-approved.txt").exists(),
        "file must not be created before approval"
    );
}

#[test]
fn gateway_deny_clears_the_pending_request() {
    let home = tempdir().expect("tempdir");
    let server = MockServer::start();
    script_create_file_flow(&server);
    configure_provider(home.path(), &server.base_url());

    // Park a request.
    run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "please create gw-approved.txt",
        ],
    );
    // Deny it.
    let denied = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "/deny",
        ],
    );
    assert!(denied.status.success(), "stderr: {}", stderr(&denied));
    assert!(
        stdout(&denied).contains("Denied"),
        "reply: {}",
        stdout(&denied)
    );
    assert!(
        !home.path().join("gw-approved.txt").exists(),
        "denied tool must never run"
    );
    // A second /deny finds nothing pending.
    let again = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "/deny",
        ],
    );
    assert!(
        stdout(&again).contains("No permission request is pending"),
        "reply: {}",
        stdout(&again)
    );
}

#[test]
fn gateway_approve_runs_the_privileged_tool() {
    let home = tempdir().expect("tempdir");
    let server = MockServer::start();
    script_create_file_flow(&server);
    configure_provider(home.path(), &server.base_url());

    // Park a request.
    let parked = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "please create gw-approved.txt",
        ],
    );
    assert!(parked.status.success(), "stderr: {}", stderr(&parked));
    assert!(!home.path().join("gw-approved.txt").exists());

    // Approve it: the turn resumes and the granted tool finally runs.
    let approved = run(
        home.path(),
        &[
            "gateway",
            "message",
            "--conversation",
            "C1",
            "--user",
            "U1",
            "/approve",
        ],
    );
    assert!(approved.status.success(), "stderr: {}", stderr(&approved));
    let created = home.path().join("gw-approved.txt");
    assert!(created.exists(), "approved tool must create the file");
    assert_eq!(
        std::fs::read_to_string(&created).expect("read created file"),
        "written-by-gateway"
    );
}
