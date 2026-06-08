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
