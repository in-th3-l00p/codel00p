//! The default interactive tool set makes the agent fully capable.
//!
//! codel00p is a coding agent. An `agent run` with **no `--tool-set` flag** is
//! fully capable: the model is advertised the read, edit, command, and git tools
//! (and more) without the user opting in. `--tool-set` only *restricts*. These
//! scenarios pin that behavior through the real binary.
//!
//! Fully hermetic: own `CODEL00P_HOME`, workspace tempdir, and `memory.sqlite`.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

#[test]
fn default_run_advertises_all_tools() {
    // No tool call needed — a single final-text turn exercises the manifest.
    let provider = MockProvider::start().assistant_text("Ready.");

    let runner = CodelRunner::new().with_provider(&provider);
    // Deliberately NO `--tool-set` argument: this is the default the user hits.
    let result = runner.run(&["agent", "run", "What can you do?"]);

    result.assert_success();
    result.assert_turn_completed();

    let manifest = result.assert_context_manifest();
    if let AgentEvent::ContextManifest {
        advertised_tools, ..
    } = manifest
    {
        // The default is fully capable: read + edit + command + git, no flag.
        for expected in &[
            "read_file",
            "create_file",
            "update_file",
            "delete_file",
            "apply_patch",
            "run_command",
            "git_status",
        ] {
            assert!(
                advertised_tools.iter().any(|t| t == *expected),
                "default run must advertise '{expected}' (fully capable by default), \
                 got: {advertised_tools:?}"
            );
        }
    } else {
        panic!("assert_context_manifest returned a non-ContextManifest event");
    }
}

#[test]
fn default_run_can_create_a_file_without_tool_set_flag() {
    // The exact user-reported flow: "create and write to a file" with no
    // `--tool-set`. The model calls create_file and the file must land on disk.
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "notes.txt", "content": "written by the agent\n" }),
        )
        .assistant_text("Created notes.txt.");

    let runner = CodelRunner::new().with_provider(&provider);
    let result = runner.run(&["agent", "run", "Create notes.txt and write to it."]);

    result.assert_success();
    result.assert_tool_called("create_file");
    result.assert_turn_completed();

    let file_path = runner.workspace_path().join("notes.txt");
    assert!(
        file_path.exists(),
        "notes.txt should have been created by the default tool set"
    );
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "written by the agent\n",
        "notes.txt contents should match what the agent wrote"
    );
}
