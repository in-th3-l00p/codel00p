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
        .current_dir(home)
        .args(args)
        .output()
        .expect("run codel00p")
}

/// Like `run`, but with a provider credential set so an agent turn can resolve.
fn run_agent(home: &Path, args: &[&str]) -> Output {
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

#[test]
fn cron_list_is_empty_by_default() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["cron", "list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("No scheduled jobs"));
}

#[test]
fn cron_add_list_show_remove_round_trip() {
    let home = tempdir().expect("tempdir");

    let add = run(home.path(), &["cron", "add", "30m", "Run", "the", "checks"]);
    assert!(add.status.success(), "stderr: {}", stderr(&add));
    assert!(stdout(&add).contains("Added cron-1 (every 30m)"));

    let list = run(home.path(), &["cron", "list"]);
    let listed = stdout(&list);
    assert!(listed.contains("cron-1"), "list: {listed}");
    assert!(listed.contains("every 30m"), "list: {listed}");

    let show = run(home.path(), &["cron", "show", "cron-1"]);
    let shown = stdout(&show);
    assert!(shown.contains("schedule:  every 30m"), "show: {shown}");
    assert!(shown.contains("Run the checks"), "show: {shown}");

    let disable = run(home.path(), &["cron", "disable", "cron-1"]);
    assert!(stdout(&disable).contains("Disabled cron-1"));

    let remove = run(home.path(), &["cron", "remove", "cron-1"]);
    assert!(stdout(&remove).contains("Removed cron-1"));
    assert!(stdout(&run(home.path(), &["cron", "list"])).contains("No scheduled jobs"));
}

#[test]
fn cron_add_rejects_bad_schedule() {
    let home = tempdir().expect("tempdir");
    let output = run(home.path(), &["cron", "add", "soon", "do it"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("invalid schedule"));
}

#[test]
fn cron_run_executes_a_job_as_an_agent_turn() {
    let home = tempdir().expect("tempdir");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "ran the nightly job" }, "finish_reason": "stop" }
            ]
        }));
    });

    // Point the agent at the mock provider via config.
    assert!(
        run(home.path(), &["config", "set", "agent.provider", "custom"])
            .status
            .success()
    );
    assert!(
        run(home.path(), &["config", "set", "agent.model", "test-model"])
            .status
            .success()
    );
    assert!(
        run(
            home.path(),
            &["config", "set", "agent.base_url", &server.base_url()]
        )
        .status
        .success()
    );

    let add = run(
        home.path(),
        &["cron", "add", "1h", "summarize", "the", "day"],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    let output = run_agent(home.path(), &["cron", "run", "cron-1"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("ran the nightly job"));
    provider.assert();
}

#[test]
fn cron_run_refuses_a_disabled_job() {
    let home = tempdir().expect("tempdir");
    assert!(
        run(home.path(), &["config", "set", "agent.provider", "custom"])
            .status
            .success()
    );
    assert!(
        run(home.path(), &["config", "set", "agent.model", "m"])
            .status
            .success()
    );
    assert!(
        run(home.path(), &["cron", "add", "1h", "do it"])
            .status
            .success()
    );
    assert!(
        run(home.path(), &["cron", "disable", "cron-1"])
            .status
            .success()
    );

    let output = run_agent(home.path(), &["cron", "run", "cron-1"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("disabled"));
}

#[test]
fn cron_daemon_once_runs_due_jobs_then_tracks_state() {
    let home = tempdir().expect("tempdir");

    let server = MockServer::start();
    let provider = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).json_body(json!({
            "choices": [
                { "message": { "role": "assistant", "content": "did the work" }, "finish_reason": "stop" }
            ]
        }));
    });

    assert!(
        run(home.path(), &["config", "set", "agent.provider", "custom"])
            .status
            .success()
    );
    assert!(
        run(home.path(), &["config", "set", "agent.model", "test-model"])
            .status
            .success()
    );
    assert!(
        run(
            home.path(),
            &["config", "set", "agent.base_url", &server.base_url()]
        )
        .status
        .success()
    );
    assert!(
        run(home.path(), &["cron", "add", "1h", "nightly", "sweep"])
            .status
            .success()
    );

    // First tick: the never-run job is due and executes.
    let first = run_agent(home.path(), &["cron", "daemon", "--once"]);
    assert!(first.status.success(), "stderr: {}", stderr(&first));
    assert!(
        stdout(&first).contains("Ran 1 job(s): cron-1"),
        "out: {}",
        stdout(&first)
    );
    assert!(provider.calls() >= 1);

    // Second tick right away: it just ran, so nothing is due (state tracked).
    let second = run_agent(home.path(), &["cron", "daemon", "--once"]);
    assert!(second.status.success(), "stderr: {}", stderr(&second));
    assert!(
        stdout(&second).contains("No jobs due"),
        "out: {}",
        stdout(&second)
    );

    // The run is recorded on the job.
    assert!(!stdout(&run(home.path(), &["cron", "show", "cron-1"])).contains("never"));
}
