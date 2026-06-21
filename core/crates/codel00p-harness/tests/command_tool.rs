use std::fs;

use codel00p_harness::{HarnessError, PermissionScope, ToolRegistry, Workspace};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn command_defaults_expose_shell_scoped_run_command_tool() {
    let registry = ToolRegistry::command_defaults();

    assert_eq!(
        registry.names(),
        vec![
            "process_kill",
            "process_list",
            "process_output",
            "run_checks",
            "run_command"
        ]
    );
    assert_eq!(
        registry.permission_scope("run_command", &json!({})),
        PermissionScope::Shell
    );
    // The project-aware check runner executes commands → shell-scoped, just
    // like run_command.
    assert_eq!(
        registry.permission_scope("run_checks", &json!({})),
        PermissionScope::Shell
    );
    // The polling tools are read-only; killing is shell-scoped.
    assert_eq!(
        registry.permission_scope("process_output", &json!({})),
        PermissionScope::ReadOnly
    );
    assert_eq!(
        registry.permission_scope("process_kill", &json!({})),
        PermissionScope::Shell
    );
}

#[tokio::test]
async fn run_command_executes_program_in_workspace_and_reports_output() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::command_defaults();

    let result = registry
        .execute(
            "run_command",
            &workspace,
            json!({
                "program": "sh",
                "args": ["-c", "printf stdout; printf stderr >&2; test -f README.md"],
                "timeout_ms": 1000
            }),
        )
        .await
        .expect("run command");

    assert_eq!(
        result.content(),
        &json!({
            "program": "sh",
            "args": ["-c", "printf stdout; printf stderr >&2; test -f README.md"],
            "cwd": ".",
            "exit_code": 0,
            "success": true,
            "timed_out": false,
            "stdout": "stdout",
            "stderr": "stderr",
            "stdout_truncated": false,
            "stderr_truncated": false
        })
    );
}

#[tokio::test]
async fn run_command_constrains_working_directory_to_workspace() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::command_defaults();

    let error = registry
        .execute(
            "run_command",
            &workspace,
            json!({
                "program": "pwd",
                "cwd": "../",
                "timeout_ms": 1000
            }),
        )
        .await
        .expect_err("workspace escape should fail");

    assert!(matches!(error, HarnessError::WorkspaceEscape { .. }));
}

#[tokio::test]
async fn run_command_reports_nonzero_exit_codes_without_failing_tool() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::command_defaults();

    let result = registry
        .execute(
            "run_command",
            &workspace,
            json!({
                "program": "sh",
                "args": ["-c", "printf nope >&2; exit 7"],
                "timeout_ms": 1000
            }),
        )
        .await
        .expect("run command");

    assert_eq!(result.content()["exit_code"], 7);
    assert_eq!(result.content()["success"], false);
    assert_eq!(result.content()["timed_out"], false);
    assert_eq!(result.content()["stderr"], "nope");
}

#[tokio::test]
async fn run_command_times_out_and_marks_result() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::command_defaults();

    let result = registry
        .execute(
            "run_command",
            &workspace,
            json!({
                "program": "sh",
                "args": ["-c", "sleep 1"],
                "timeout_ms": 50
            }),
        )
        .await
        .expect("run command");

    assert_eq!(result.content()["exit_code"], json!(null));
    assert_eq!(result.content()["success"], false);
    assert_eq!(result.content()["timed_out"], true);
}

#[tokio::test]
async fn run_command_caps_stdout_and_stderr() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let registry = ToolRegistry::command_defaults();

    let result = registry
        .execute(
            "run_command",
            &workspace,
            json!({
                "program": "sh",
                "args": ["-c", "printf 1234567890; printf abcdefghij >&2"],
                "timeout_ms": 1000,
                "max_output_bytes": 4
            }),
        )
        .await
        .expect("run command");

    assert_eq!(result.content()["stdout"], "1234");
    assert_eq!(result.content()["stderr"], "abcd");
    assert_eq!(result.content()["stdout_truncated"], true);
    assert_eq!(result.content()["stderr_truncated"], true);
}
