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
fn bare_invocation_opens_chat() {
    // Bare `codel00p` (no subcommand) must route to the interactive chat — the
    // primary UI — not the old "missing command" error. With empty stdin the
    // chat either starts its banner (provider configured) or reports the missing
    // provider (not configured); both prove it reached the chat path.
    let home = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_codel00p"))
        .env("CODEL00P_HOME", home.path())
        .current_dir(home.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run codel00p");
    let combined = String::from_utf8_lossy(&output.stderr).to_string() + &stdout(&output);
    assert!(
        combined.contains("no provider configured")
            || combined.contains("Type a message")
            || combined.contains("codel00p chat"),
        "bare invocation should enter chat; got: {combined}"
    );
    assert!(
        !combined.contains("missing command"),
        "bare invocation must not error; got: {combined}"
    );
}

#[test]
fn top_level_help_prints_without_project_flags() {
    let output = run_codel00p(&["--help"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let help = stdout(&output);
    assert!(help.contains("Usage"));
    assert!(help.contains("codel00p [options] [command]"));
    assert!(help.contains("open the interactive chat (default)"));
    assert!(help.contains("agent      Run the agent"));
    assert!(help.contains("config     Settings, providers, and plugins"));
    assert!(help.contains("auth       Sign in or out of the codel00p cloud"));
    assert!(help.contains("mcp        Expose codel00p as an MCP server"));
    assert!(help.contains("memory     Review project memory"));
    assert!(help.contains("session    Inspect persisted sessions"));
    // providers and plugins moved under config; login/logout under auth.
    assert!(!help.contains("\nlogin "), "login should not be top-level");
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
            &["agent", "chat", "--help"][..],
            "Usage: codel00p [global options] agent chat [options]",
        ),
        (
            &["agent", "mcp", "--help"][..],
            "Usage: codel00p [global options] agent mcp <command>",
        ),
        (
            &["agent", "mcp", "list", "--help"][..],
            "Usage: codel00p [global options] agent mcp list",
        ),
        (
            &["agent", "mcp", "doctor", "--help"][..],
            "Usage: codel00p [global options] agent mcp doctor",
        ),
        (
            &["mcp", "--help"][..],
            "Usage: codel00p [global options] mcp <command>",
        ),
        (
            &["mcp", "serve", "--help"][..],
            "Usage: codel00p [global options] mcp serve",
        ),
        (
            &["mcp", "permissions", "--help"][..],
            "Usage: codel00p [global options] mcp permissions <command>",
        ),
        (
            &["mcp", "permissions", "list", "--help"][..],
            "Usage: codel00p [global options] mcp permissions list",
        ),
        (
            &["mcp", "permissions", "forget", "--help"][..],
            "Usage: codel00p [global options] mcp permissions forget <tool-name>",
        ),
        (
            &["memory", "--help"][..],
            "Usage: codel00p [global options] memory [command]",
        ),
        (
            &["session", "--help"][..],
            "Usage: codel00p [global options] sessions [command]",
        ),
        (
            &["config", "--help"][..],
            "Usage: codel00p config <command>",
        ),
        (
            &["config", "providers", "--help"][..],
            "Usage: codel00p config providers <command>",
        ),
        (
            &["config", "plugins", "--help"][..],
            "Usage: codel00p config plugins <command>",
        ),
        (&["auth", "--help"][..], "Usage: codel00p auth <command>"),
        (
            &["auth", "login", "--help"][..],
            "Usage: codel00p auth login [options]",
        ),
        (
            &["auth", "logout", "--help"][..],
            "Usage: codel00p auth logout",
        ),
        (
            &["session", "list", "--help"][..],
            "Usage: codel00p [global options] session list [--json]",
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
        assert!(
            stdout(&output).contains("--remember-permissions"),
            "args: {args:?}, stdout: {}",
            stdout(&output)
        );
        assert!(
            stdout(&output).contains("--mcp-server <id=command>"),
            "args: {args:?}, stdout: {}",
            stdout(&output)
        );
    }
}

#[test]
fn memory_help_documents_edit_command() {
    let output = run_codel00p(&["memory", "--help"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("edit     Edit memory content; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains(
            "restore  Restore content from an edit audit sequence; use --json for JSON output"
        ),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output)
            .contains("similar  Score active near-duplicate memory; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("quality  List active memory with low advisory quality scores"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output)
            .contains("search   Search approved memory records; supports --sensitivity and --json"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("list     List memory records; supports --sensitivity and --json"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("show     Show one memory record; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("audit    Show memory audit history; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("approve  Approve candidate memory; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("reject   Reject candidate memory; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("archive  Archive memory; use --json for JSON output"),
        "stdout: {}",
        stdout(&output)
    );
}
