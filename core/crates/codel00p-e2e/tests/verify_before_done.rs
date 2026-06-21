//! End-to-end: the verify-before-done loop (perfect-coding-agent #12 T0.1/T0.2)
//! against the real `codel00p` binary with a scripted mock provider.
//!
//! The model edits a file then "finishes". A controllable verification command
//! fails on its first run and passes on its second, so the harness must re-loop
//! the turn on the failed verification and complete only once the check passes —
//! the structural guard against "green tests but a broken app". Fully hermetic.

use codel00p_e2e::{AgentEvent, CodelRunner, MockProvider};
use serde_json::json;

/// A verification script that fails the FIRST time it runs (leaving a marker)
/// and passes every time after — so the first verify attempt fails and the
/// re-loop's second attempt passes, deterministically and with no network.
const VERIFY_SH: &str = "#!/bin/sh\n\
if [ -f .verify_ran ]; then\n\
  exit 0\n\
else\n\
  touch .verify_ran\n\
  echo 'first run fails' 1>&2\n\
  exit 1\n\
fi\n";

/// Project config enabling verify-before-done with the controllable command and
/// the self-critique step off (keeps the scripted flow minimal).
const PROJECT_CONFIG: &str = "\
[agent.behavior]\n\
self_verify = true\n\
self_critique = false\n\
verify_iterations = 3\n\
test_command = \"sh verify.sh\"\n";

#[test]
fn verify_before_done_reloops_on_failure_then_completes_on_pass() {
    let runner = CodelRunner::new()
        .workspace_file("verify.sh", VERIFY_SH)
        .workspace_file(".codel00p/config.toml", PROJECT_CONFIG);

    // The model creates a file and then "finishes" (twice — the mock keys
    // responses on tool-result count, so both done-points serve the same
    // assistant turn). The harness's verification fails the first time and
    // passes the second, so the turn re-loops and only then completes.
    let provider = MockProvider::start()
        .tool_call(
            "create_file",
            json!({ "path": "feature.txt", "content": "feature\n" }),
        )
        .assistant_text("All done.");

    let runner = runner.with_provider(&provider);
    let result = runner.run(&["agent", "run", "Add the feature.", "--tool-set", "all"]);

    result.assert_success();
    result.assert_tool_called("create_file");

    // Verification ran at least twice: a failure that re-looped the turn, then a
    // pass that let it complete.
    let failed = result
        .events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::VerificationCompleted { success: false, .. }))
        .count();
    let passed = result
        .events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::VerificationCompleted { success: true, .. }))
        .count();
    assert!(
        failed >= 1,
        "a failed verification should have re-looped the turn.\nevents: {:#?}",
        result.events()
    );
    assert!(
        passed >= 1,
        "a passing verification should have let the turn complete.\nevents: {:#?}",
        result.events()
    );

    // The turn completed with the verified=true signal.
    assert!(
        result.events().iter().rev().any(|e| matches!(
            e,
            AgentEvent::TurnCompleted {
                verified: Some(true),
                ..
            }
        )),
        "the turn must complete only after verification passed.\nevents: {:#?}",
        result.events()
    );

    // The edit landed and the verification marker was created by the script.
    assert!(runner.workspace_path().join("feature.txt").exists());
    assert!(runner.workspace_path().join(".verify_ran").exists());
}
