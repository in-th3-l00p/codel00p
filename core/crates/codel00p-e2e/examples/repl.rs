//! Interactive exploration harness: launch the real `codel00p` binary against a
//! canned mock provider and stream `--json-events`, with zero secrets or network.
//!
//! Run it with:
//!
//! ```bash
//! cargo run -p codel00p-e2e --example repl -- "Create and commit notes.txt"
//! ```
//!
//! The default script drives a create_file -> git add -> git_commit -> final-text
//! loop. Pass a prompt as the first argument to change the task string the agent
//! is given (the scripted tool loop stays the same — this is a deterministic,
//! offline playground, not a live model).
//!
//! To instead point the binary at a LIVE provider, skip this example and invoke
//! the binary directly, e.g.:
//!
//! ```bash
//! CODEL00P_PROVIDER_OPENAI_API_KEY=sk-... \
//!   codel00p agent run "..." --provider openai --model gpt-5 \
//!   --tool-set all --json-events
//! ```

use codel00p_e2e::{CodelRunner, MockProvider};
use serde_json::json;

fn main() {
    let prompt = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Create and commit notes.txt".to_string());

    let runner = CodelRunner::new()
        .workspace_file("README.md", "# scratch\n")
        .git_init();

    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "notes.txt", "content": "scripted note\n" }),
        )
        .tool_call(
            "run_command",
            json!({ "program": "git", "args": ["add", "notes.txt"] }),
        )
        .tool_call("git_commit", json!({ "message": "chore: add notes.txt" }))
        .assistant_text("Done.\nremember workflow[repl]: notes live in notes.txt.");

    let runner = runner.with_provider(&provider);

    println!("workspace: {}", runner.workspace_path().display());
    println!("home:      {}", runner.home_path().display());
    println!("prompt:    {prompt}\n");

    let result = runner.run(&["agent", "run", &prompt, "--tool-set", "all"]);

    println!(
        "--- stdout (final text + json events) ---\n{}",
        result.stdout()
    );
    if !result.stderr().is_empty() {
        println!("--- stderr ---\n{}", result.stderr());
    }
    println!("--- parsed events ---");
    for event in result.events() {
        println!("{event:?}");
    }
    println!("\nexit: success={}", result.success());
}
