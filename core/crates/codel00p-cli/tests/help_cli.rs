use std::process::{Command, Output};

fn run_codel00p(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codel00p"))
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
fn top_level_help_prints_without_project_flags() {
    let output = run_codel00p(&["--help"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let help = stdout(&output);
    assert!(help.contains("Usage: codel00p [global options] <command>"));
    assert!(help.contains("agent    Run the coding agent"));
    assert!(help.contains("memory   Review project memory"));
    assert!(help.contains("session  Inspect persisted sessions"));
}

#[test]
fn command_help_prints_without_project_flags() {
    for (args, expected) in [
        (
            &["agent", "--help"][..],
            "Usage: codel00p [global options] agent <command>",
        ),
        (
            &["agent", "run", "--help"][..],
            "Usage: codel00p [global options] agent run <prompt>",
        ),
        (
            &["agent", "resume", "--help"][..],
            "Usage: codel00p [global options] agent resume <session-id> <prompt>",
        ),
        (
            &["memory", "--help"][..],
            "Usage: codel00p [global options] memory <command>",
        ),
        (
            &["session", "--help"][..],
            "Usage: codel00p [global options] session <command>",
        ),
    ] {
        let output = run_codel00p(args);

        assert!(
            output.status.success(),
            "args: {args:?}, stderr: {}",
            stderr(&output)
        );
        assert!(
            stdout(&output).contains(expected),
            "args: {args:?}, stdout: {}",
            stdout(&output)
        );
    }
}

#[test]
fn agent_help_documents_tool_set_opt_in() {
    for args in [
        &["agent", "run", "--help"][..],
        &["agent", "resume", "--help"][..],
    ] {
        let output = run_codel00p(args);

        assert!(
            output.status.success(),
            "args: {args:?}, stderr: {}",
            stderr(&output)
        );
        assert!(
            stdout(&output).contains("--tool-set <name>"),
            "args: {args:?}, stdout: {}",
            stdout(&output)
        );
        assert!(
            stdout(&output).contains("--stream-events"),
            "args: {args:?}, stdout: {}",
            stdout(&output)
        );
        assert!(
            stdout(&output).contains("--permission-mode <mode>"),
            "args: {args:?}, stdout: {}",
            stdout(&output)
        );
    }
}
