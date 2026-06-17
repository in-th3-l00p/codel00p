//! Pilot full-stack e2e scenario.
//!
//! Drives the real `codel00p` binary through a multi-iteration tool loop against
//! a scripted mock provider, and asserts across every layer: emitted protocol
//! events, workspace side effects, git history, persisted memory candidates, and
//! session replay.
//!
//! Fully hermetic: no network, no API keys, isolated `CODEL00P_HOME` + workspace.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use codel00p_memory::{MemoryListFilter, MemoryRepository, StorageBackedMemoryStore};
use codel00p_protocol::{MemoryStatus, ProjectRef};
use codel00p_storage::{SqliteStorage, StorageScope};
use serde_json::json;

#[test]
fn pilot_full_stack_tool_loop_with_side_effects_memory_and_replay() {
    // 1. Isolated world: seed a file FIRST, then git-init so the seeded file is
    //    captured in the initial commit (leaving the tree clean — important
    //    because git_commit refuses to run while untracked files are present).
    let runner = CodelRunner::new()
        .workspace_file("README.md", "# pilot\n")
        .git_init();

    // 2. Script a multi-iteration loop. The agent re-calls the model after each
    //    tool result, so these turns are served in order:
    //      turn 0 -> create_file (new untracked file)
    //      turn 1 -> run_command (`git add pilot.txt`) — stages it so the commit
    //                tool, which refuses to commit while untracked files are
    //                present and only commits tracked changes, has something to do
    //      turn 2 -> git_commit
    //      turn 3 -> final assistant text containing a `remember:` directive.
    //    Memory extraction + a candidate sink ARE wired in the `agent run` CLI
    //    path (see codel00p-cli/src/agent/turn.rs::build_agent_harness_with), so
    //    the directive becomes a persisted candidate.
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "pilot.txt", "content": "hello from pilot\n" }),
        )
        .tool_call(
            "run_command",
            json!({ "program": "git", "args": ["add", "pilot.txt"] }),
        )
        .tool_call("git_commit", json!({ "message": "feat: add pilot.txt" }))
        .assistant_text(
            "All done.\nremember convention[pilot]: pilot.txt holds the pilot greeting.",
        );

    let runner = runner.with_provider(&provider);

    // 3. Run the agent with the full tool set.
    let result = runner.run(&[
        "agent",
        "run",
        "Create and commit pilot.txt.",
        "--tool-set",
        "all",
    ]);
    result.assert_success();

    // 4. Event-layer assertions: each scripted tool ran, the turn finished, and a
    //    context manifest was emitted.
    result.assert_tool_called("create_file");
    result.assert_tool_called("run_command");
    result.assert_tool_called("git_commit");
    result.assert_turn_completed();
    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        assert!(
            advertised_tools.iter().any(|t| t == "create_file"),
            "manifest should advertise create_file, got {advertised_tools:?}"
        );
    }

    // The mock served the expected number of model round-trips (4 turns).
    assert_eq!(provider.hits(), 4, "expected 4 model round-trips");

    // 5. Side-effect assertions: the file exists with the scripted contents, and
    //    git history shows the commit.
    let pilot = runner.workspace_path().join("pilot.txt");
    assert!(pilot.exists(), "pilot.txt should have been created");
    assert_eq!(
        std::fs::read_to_string(&pilot).unwrap(),
        "hello from pilot\n"
    );
    let log = git_log_subjects(runner.workspace_path());
    assert!(
        log.iter().any(|s| s == "feat: add pilot.txt"),
        "git log should contain the commit, got {log:?}"
    );

    // 6. Memory assertion (memory extraction IS wired in `agent run`): a
    //    candidate was persisted from the `remember:` directive. Read it back
    //    through the real memory repository types.
    let candidates = list_memory_candidates(&runner.memory_db());
    assert!(
        candidates
            .iter()
            .any(|c| c.contains("pilot.txt holds the pilot greeting")),
        "expected a persisted memory candidate from the remember directive, got {candidates:?}"
    );

    // 7. Replay: a second invocation on the SAME home reconstructs the session.
    //    `session list --json` reports a persisted session; `session show`
    //    replays the prior user message.
    let sessions = runner.run(&["session", "list", "--json"]);
    sessions.assert_success();
    let parsed: serde_json::Value = serde_json::from_str(sessions.stdout().trim())
        .expect("session list --json should be valid JSON");
    let session_id = parsed
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("session_id"))
        .and_then(|v| v.as_str())
        .expect("at least one persisted session with an id")
        .to_string();

    let shown = runner.run(&["session", "show", &session_id]);
    shown.assert_success();
    assert!(
        shown.stdout().contains("Create and commit pilot.txt."),
        "session replay should contain the original user prompt, got:\n{}",
        shown.stdout()
    );
}

/// Returns git commit subjects (newest first) for the given repo.
fn git_log_subjects(repo: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo)
        .args(["log", "--pretty=format:%s"])
        .output()
        .expect("run git log");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Reads persisted memory candidates through the real repository types.
fn list_memory_candidates(db: &std::path::Path) -> Vec<String> {
    let storage = SqliteStorage::open(db).expect("open memory sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let project = ProjectRef::new("project-1", "codel00p");
    let filter = MemoryListFilter::new(project).with_status(MemoryStatus::Candidate);
    store
        .list(filter)
        .expect("list memory candidates")
        .into_iter()
        .map(|record| record.entry().content().to_string())
        .collect()
}
