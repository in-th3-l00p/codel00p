//! End-to-end scenarios for codel00p's `--permission-mode` policy.
//!
//! These drive the real `codel00p` binary through `agent run` against a scripted
//! mock provider and assert across two layers: the emitted `--json-events`
//! permission events ([`AgentEvent::PermissionRequested`] /
//! [`AgentEvent::PermissionDenied`]) and the workspace side effects a write/shell
//! tool would (or would not) produce.
//!
//! ## What the CLI actually does (verified from source, not assumed)
//!
//! `--permission-mode` accepts three modes (see
//! `codel00p-cli/src/agent/options.rs::parse_permission_mode`, case-insensitive):
//!   - `allow` / `allowed`  → every tool is auto-approved.
//!   - `ask`   / `prompt` / `interactive` → each privileged tool prompts on
//!     stdin (`y/N`). With no remembered decision it reaches
//!     `prompt_for_permission`, which reads a line from stdin.
//!   - `deny`  / `denied`   → every tool is auto-denied with the message
//!     "`<tool>` denied by CLI permission mode".
//!
//! Permission decisioning lives in `codel00p-cli/src/agent/permissions.rs`
//! (`CliPermissionPolicy`). The harness turn loop
//! (`codel00p-harness/src/agent/turn.rs`) always emits a `PermissionRequested`
//! event for *every* tool call (even in `allow` mode and for read-only tools),
//! then emits `PermissionDenied` only when the policy refuses execution. A denied
//! tool is fed an error result (an `Ok(ToolResult::json({error, error_kind}))`)
//! back to the model and the turn continues — so we script a trailing assistant
//! text turn to let it finish gracefully. Note the error result is still an `Ok`
//! tool result, so the turn loop *does* emit a `ToolCallCompleted` for a denied
//! tool; the load-bearing signals are the `PermissionDenied` event and the
//! absence of the side effect, which is what these tests assert.
//!
//! ### Non-interactive `ask`
//!
//! Under `agent run` (no TTY), `Command::output()` gives the child a closed
//! stdin, so `prompt_for_permission`'s `read_line` returns 0 bytes (EOF) and is
//! treated as "no" → the tool is **auto-denied** with the message "`<tool>`
//! rejected by CLI approval prompt". Read-only tools never prompt. So
//! non-interactive `ask` behaves like `deny` for privileged tools, but via the
//! prompt path (different deny message), which the `ask` test asserts.
//!
//! ### `--remember-permissions`
//!
//! Remembering is only wired for tools where
//! `is_rememberable_permission(name, scope)` holds
//! (`codel00p-cli/src/connector_permissions.rs`): `mcp.*` tools or the
//! `ExternalConnector` scope. The edit/command tools used here are
//! `WorkspaceWrite` / `Shell` scope, so their decisions are never persisted and
//! `--remember-permissions` has no observable effect on them. We assert exactly
//! that: after an `ask --remember-permissions` run that denies `create_file`, the
//! connector-permission store is still empty. (Cross-run reuse of a *remembered*
//! decision is only observable for MCP/external-connector tools, which would
//! require standing up an MCP server — out of scope for this hermetic edit/shell
//! path, so it is documented here rather than asserted.)

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use codel00p_storage::{KeyValueStore, SqliteStorage, StorageScope};
use serde_json::json;

/// Counts `connector_permission:` entries persisted in a run's memory db.
fn remembered_permission_count(db: &std::path::Path) -> usize {
    let storage = SqliteStorage::open(db).expect("open memory sqlite");
    let scope = StorageScope::project("org-1", "project-1");
    storage
        .list_values(&scope, Some("connector_permission:"))
        .expect("list connector permissions")
        .len()
}

/// allow mode: a write tool executes, the file is created, and no
/// `PermissionDenied` event fires. (A `PermissionRequested` event *does* fire —
/// the turn loop emits one for every tool call regardless of mode — so we assert
/// the absence of a *denial*, not the absence of a request.)
#[test]
fn allow_mode_executes_write_tool_with_no_denial() {
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "allowed.txt", "content": "allowed by policy\n" }),
        )
        .assistant_text("Created the file.");

    let runner = CodelRunner::new()
        .permission_mode("allow")
        .with_provider(&provider);

    let result = runner.run(&["agent", "run", "Create allowed.txt.", "--tool-set", "edit"]);
    result.assert_success();

    // The write tool ran end-to-end and the turn finished.
    result.assert_tool_called("create_file");
    result.assert_turn_completed();

    // No denial event was emitted.
    assert!(
        !result.has_event(|event| matches!(event, AgentEvent::PermissionDenied { .. })),
        "allow mode must not emit any PermissionDenied event.\nevents: {:#?}",
        result.events()
    );

    // Side effect: the file exists with the scripted contents.
    let created = runner.workspace_path().join("allowed.txt");
    assert!(created.exists(), "allowed.txt should have been created");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "allowed by policy\n"
    );
}

/// deny mode: a write tool is requested but auto-denied. A `PermissionRequested`
/// AND a `PermissionDenied` event fire for it, the file is NOT created, and the
/// turn still completes gracefully (the model is re-called with the denial result
/// and returns final text).
#[test]
fn deny_mode_blocks_write_tool_and_skips_side_effect() {
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "denied.txt", "content": "should never be written\n" }),
        )
        .assistant_text("Understood, I will not create the file.");

    let runner = CodelRunner::new()
        .permission_mode("deny")
        .with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Try to create denied.txt.",
        "--tool-set",
        "edit",
    ]);
    // The turn completes gracefully even though the tool was blocked.
    result.assert_success();
    result.assert_turn_completed();

    // The permission was requested and then denied for create_file.
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionRequested { tool_name, .. } if tool_name == "create_file"
        )
    });
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionDenied { tool_name, message, .. }
                if tool_name == "create_file"
                    && message.contains("denied by CLI permission mode")
        )
    });

    // Side effect did NOT happen.
    assert!(
        !runner.workspace_path().join("denied.txt").exists(),
        "denied.txt must NOT exist when create_file was denied"
    );

    // The model was re-called after the denial (initial request + post-denial
    // round-trip = 2 hits), confirming graceful continuation.
    assert_eq!(
        provider.hits(),
        2,
        "expected the model to be re-called after the denial"
    );
}

/// deny mode also blocks a shell command: the command never runs, so its side
/// effect (a file `touch` would create) is absent.
#[test]
fn deny_mode_blocks_shell_command_side_effect() {
    let provider = MockProvider::start()
        .tool_call(
            "run_command",
            json!({ "program": "touch", "args": ["command-ran.txt"] }),
        )
        .assistant_text("I cannot run that command.");

    let runner = CodelRunner::new()
        .permission_mode("deny")
        .with_provider(&provider);

    let result = runner.run(&[
        "agent",
        "run",
        "Try to touch a file.",
        "--tool-set",
        "command",
    ]);
    result.assert_success();
    result.assert_turn_completed();

    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionDenied { tool_name, message, .. }
                if tool_name == "run_command"
                    && message.contains("denied by CLI permission mode")
        )
    });

    // The shell command never ran, so its file was never created.
    assert!(
        !runner.workspace_path().join("command-ran.txt").exists(),
        "command-ran.txt must NOT exist when run_command was denied"
    );
}

/// ask mode in a non-interactive `agent run`: with no TTY, the stdin `y/N` prompt
/// reads EOF and is treated as "no", so a privileged tool is auto-denied via the
/// *prompt* path. We assert the prompt-specific deny message (distinct from the
/// `deny`-mode message) and that the side effect did not happen.
#[test]
fn ask_mode_non_interactive_auto_denies_via_prompt() {
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "asked.txt", "content": "should not be written\n" }),
        )
        .assistant_text("No approval was given, so I stopped.");

    let runner = CodelRunner::new()
        .permission_mode("ask")
        .with_provider(&provider);

    let result = runner.run(&["agent", "run", "Create asked.txt.", "--tool-set", "edit"]);
    result.assert_success();
    result.assert_turn_completed();

    // Auto-denied through the approval prompt path (EOF on stdin => "no"). The
    // message comes from `decide_with_prompt`, distinguishing it from deny mode.
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionDenied { tool_name, message, .. }
                if tool_name == "create_file"
                    && message.contains("rejected by CLI approval prompt")
        )
    });

    // Side effect did NOT happen.
    assert!(
        !runner.workspace_path().join("asked.txt").exists(),
        "asked.txt must NOT exist when ask mode auto-denied the tool"
    );
}

/// `--remember-permissions` is a no-op for non-connector tools: `create_file`
/// (WorkspaceWrite scope) is not rememberable, so after an `ask
/// --remember-permissions` run that denies it, nothing is persisted to the
/// connector-permission store. (Remembered-decision reuse is only observable for
/// `mcp.*` / ExternalConnector tools; see this file's module docs.)
#[test]
fn remember_permissions_does_not_persist_non_connector_decisions() {
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "remembered.txt", "content": "x\n" }),
        )
        .assistant_text("Stopped without approval.");

    let runner = CodelRunner::new()
        .permission_mode("ask")
        .with_provider(&provider);

    // `--remember-permissions` is a boolean flag; it is harmless alongside the
    // runner's auto-appended provider flags.
    let result = runner.run(&[
        "agent",
        "run",
        "Create remembered.txt.",
        "--tool-set",
        "edit",
        "--remember-permissions",
    ]);
    result.assert_success();

    // The decision was still a denial (no remembered allow to short-circuit it).
    result.assert_event(|event| {
        matches!(
            event,
            AgentEvent::PermissionDenied { tool_name, .. } if tool_name == "create_file"
        )
    });
    assert!(
        !runner.workspace_path().join("remembered.txt").exists(),
        "remembered.txt must NOT exist"
    );

    // Nothing was persisted: edit-tool scope is not rememberable.
    assert_eq!(
        remembered_permission_count(&runner.memory_db()),
        0,
        "non-connector permission decisions must not be remembered"
    );
}
