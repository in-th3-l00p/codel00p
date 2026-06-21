//! Verify-before-done loop + self-critique (perfect-coding-agent #12 T0.1/T0.2).
//!
//! These exercise the structural guarantee that the agent cannot declare success
//! after a mutating turn without the project's checks actually passing — the fix
//! for "green unit tests while the app is broken". The scripted model client
//! drives deterministic done-points; a controllable `run_checks` command
//! (`test -f fixed`, shell-free) fails until the model creates the sentinel.

mod support;

use std::fs;

use codel00p_harness::{
    AgentHarness, HarnessEvent, HarnessInferenceResponse, ModelToolCall, ToolRegistry, UserMessage,
    VerifyConfig, Workspace,
};
use codel00p_protocol::SessionId;
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

/// A registry with the editing tools (create_file — a mutating tool) plus the
/// command tools (which include `run_checks`).
fn editing_and_command_tools() -> ToolRegistry {
    ToolRegistry::editing_defaults().with_registry(ToolRegistry::command_defaults())
}

/// Config with verification on (test only) and self-critique off, using a
/// controllable command override so the check is deterministic and shell-free.
fn verify_only(command: &str) -> VerifyConfig {
    VerifyConfig {
        self_verify: true,
        auto_test: true,
        lint_and_fix: false,
        self_critique: false,
        verify_iterations: 3,
        test_command: Some(command.to_string()),
    }
}

fn create_file_call(id: &str, path: &str, content: &str) -> ModelToolCall {
    ModelToolCall::new(
        id,
        "create_file",
        json!({ "path": path, "content": content }),
    )
}

/// Count `VerificationCompleted` events with the given success verdict.
fn verifications(events: &[HarnessEvent], success: bool) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(event, HarnessEvent::VerificationCompleted { success: s, .. } if *s == success)
        })
        .count()
}

fn turn_completed_verified(events: &[HarnessEvent]) -> Option<bool> {
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            HarnessEvent::TurnCompleted { verified, .. } => Some(*verified),
            _ => None,
        })
        .flatten()
}

/// The centerpiece: a mutating turn "finishes", checks FAIL → the loop does NOT
/// complete; failure feedback is fed back; the model gets another turn; it makes
/// the fix; checks PASS → completes only after green.
#[tokio::test]
async fn verify_catches_failure_then_completes_after_fix() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        // 1. Mutating turn, then "done" (no tool calls) — but `fixed` is absent
        //    so verification (`test -f fixed`) will fail.
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "broken\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "All done!"),
        // 2. After the failure feedback, the model fixes it by creating `fixed`.
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c2", "fixed", "ok\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Now it's really done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(verify_only("test -f fixed"))
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("verify-fail-then-fix"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    // The first done-signal must NOT have completed the turn: a failed
    // verification ran, then a passing one.
    assert_eq!(
        verifications(&outcome.events, false),
        1,
        "exactly one failed verification was fed back"
    );
    assert_eq!(
        verifications(&outcome.events, true),
        1,
        "exactly one passing verification let the turn complete"
    );

    // The turn only completed after the fix, and the final assistant message is
    // the one that followed the successful re-verification.
    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("Now it's really done.")
    );
    assert_eq!(turn_completed_verified(&outcome.events), Some(true));

    // Both mutating tool calls ran, and the `fixed` sentinel exists.
    assert!(dir.path().join("fixed").exists());

    // The failure feedback was injected into the conversation so the model could
    // act on it.
    let has_feedback = outcome
        .session_state
        .messages()
        .iter()
        .any(|message| format!("{message:?}").contains("Automated verification failed"));
    assert!(
        has_feedback,
        "verification failure was fed back into the session"
    );

    // Four inferences: done(fail) → fix → done(pass-checked).
    assert_eq!(model.requests().len(), 4);
}

/// Pass path: a mutating turn whose checks pass completes once, with no extra
/// fix loop.
#[tokio::test]
async fn verify_pass_path_completes_without_fix_loop() {
    let dir = tempdir().expect("tempdir");
    // Sentinel already present → `test -f fixed` passes on the first run.
    fs::write(dir.path().join("fixed"), "ok\n").expect("seed sentinel");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "done\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(verify_only("test -f fixed"))
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("verify-pass"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    assert_eq!(verifications(&outcome.events, true), 1);
    assert_eq!(verifications(&outcome.events, false), 0);
    assert_eq!(outcome.assistant_message.as_deref(), Some("Done."));
    assert_eq!(turn_completed_verified(&outcome.events), Some(true));
    // No re-loop: exactly two inferences.
    assert_eq!(model.requests().len(), 2);
}

/// A read-only turn (no mutating tool) skips verification and completes
/// immediately.
#[tokio::test]
async fn read_only_turn_skips_verification() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "hi\n").expect("write");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "c1",
                "read_file",
                json!({ "path": "README.md" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Read it."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults().with_registry(ToolRegistry::command_defaults()))
        // `false` would fail if it ran — but a read-only turn must not verify.
        .verify_config(verify_only("false"))
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("read-only"),
            UserMessage::new("Read README."),
        )
        .await
        .expect("run turn");

    assert_eq!(verifications(&outcome.events, true), 0);
    assert_eq!(verifications(&outcome.events, false), 0);
    assert_eq!(outcome.assistant_message.as_deref(), Some("Read it."));
    assert_eq!(turn_completed_verified(&outcome.events), None);
    assert_eq!(model.requests().len(), 2);
}

/// No `run_checks` tool registered (command tools disabled) ⇒ verification is
/// skipped gracefully even after a mutating turn.
#[tokio::test]
async fn no_run_checks_tool_skips_gracefully() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "x\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        // Editing tools only — no run_checks registered.
        .tools(ToolRegistry::editing_defaults())
        .verify_config(verify_only("false"))
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("no-checks"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    assert_eq!(verifications(&outcome.events, true), 0);
    assert_eq!(verifications(&outcome.events, false), 0);
    assert_eq!(outcome.assistant_message.as_deref(), Some("Done."));
    assert_eq!(turn_completed_verified(&outcome.events), None);
    assert_eq!(model.requests().len(), 2);
}

/// No detectable test command and no override ⇒ verification skipped gracefully
/// (a project with no tests still completes normally).
#[tokio::test]
async fn no_detected_command_skips_gracefully() {
    let dir = tempdir().expect("tempdir");
    // No manifest (Cargo.toml/package.json/...) ⇒ detect_checks finds nothing.
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "x\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(VerifyConfig {
            self_verify: true,
            auto_test: true,
            lint_and_fix: false,
            self_critique: false,
            verify_iterations: 3,
            // No override → relies on detection, which finds nothing here.
            test_command: None,
        })
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("no-detect"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    assert_eq!(verifications(&outcome.events, true), 0);
    assert_eq!(verifications(&outcome.events, false), 0);
    assert_eq!(outcome.assistant_message.as_deref(), Some("Done."));
    assert_eq!(turn_completed_verified(&outcome.events), None);
    assert_eq!(model.requests().len(), 2);
}

/// Budget bound: checks keep failing → the loop stops at `verify_iterations` and
/// completes with the not-verified signal (no hang).
#[tokio::test]
async fn verify_budget_bounds_the_fix_loop() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    // The model keeps mutating and "finishing" but never creates `fixed`, so
    // `test -f fixed` fails every time. Supply plenty of responses; the bound
    // (2) must stop the loop well before they run out.
    let mut responses = Vec::new();
    for i in 0..8 {
        responses.push(HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call(
                &format!("c{i}"),
                &format!("attempt{i}.txt"),
                "x\n",
            )],
        ));
        responses.push(HarnessInferenceResponse::assistant(
            "github",
            "gpt-4o",
            "Done (but not really).",
        ));
    }
    let model = ScriptedModelClient::new(responses);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(VerifyConfig {
            self_verify: true,
            auto_test: true,
            lint_and_fix: false,
            self_critique: false,
            verify_iterations: 2,
            test_command: Some("test -f fixed".to_string()),
        })
        .max_iterations(30)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("verify-budget"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    // Exactly `verify_iterations` failed verifications ran, then it completed.
    assert_eq!(verifications(&outcome.events, false), 2);
    assert_eq!(verifications(&outcome.events, true), 0);
    // Completed with the not-verified signal rather than hanging or erroring.
    assert_eq!(turn_completed_verified(&outcome.events), Some(false));
    assert!(
        matches!(
            outcome.events.last(),
            Some(HarnessEvent::TurnCompleted {
                verified: Some(false),
                ..
            })
        ),
        "turn ends with a clear verification-did-not-pass signal"
    );
}

/// Toggle: `self_verify = false` ⇒ no verification at all (today's behavior); a
/// mutating turn completes immediately even though `test -f fixed` would fail.
#[tokio::test]
async fn self_verify_off_restores_legacy_behavior() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "x\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(VerifyConfig {
            self_verify: false,
            self_critique: false,
            test_command: Some("test -f fixed".to_string()),
            ..VerifyConfig::default()
        })
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("verify-off"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    assert_eq!(verifications(&outcome.events, true), 0);
    assert_eq!(verifications(&outcome.events, false), 0);
    assert_eq!(outcome.assistant_message.as_deref(), Some("Done."));
    assert_eq!(turn_completed_verified(&outcome.events), None);
    assert_eq!(model.requests().len(), 2);
}

/// Self-critique on: after a passing verification, the model gets one reflection
/// turn. If it acts (tool calls) the loop continues; here it confirms with text
/// and the turn completes, emitting the self-critique marker.
#[tokio::test]
async fn self_critique_runs_one_reflection_turn() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("fixed"), "ok\n").expect("seed sentinel");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "x\n")],
        ),
        // First "done" → verification passes → self-critique prompt injected.
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
        // Reflection response (text only) → completes.
        HarnessInferenceResponse::assistant(
            "github",
            "gpt-4o",
            "Reviewed: tests pass, change is minimal. Confirmed.",
        ),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(VerifyConfig {
            self_verify: true,
            auto_test: true,
            lint_and_fix: false,
            self_critique: true,
            verify_iterations: 3,
            test_command: Some("test -f fixed".to_string()),
        })
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("self-critique"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    assert_eq!(verifications(&outcome.events, true), 1);
    let critique = outcome
        .events
        .iter()
        .filter(|event| matches!(event, HarnessEvent::SelfCritiqueCompleted { .. }))
        .count();
    assert_eq!(critique, 1, "exactly one self-critique reflection ran");
    assert!(matches!(
        outcome
            .events
            .iter()
            .find(|e| matches!(e, HarnessEvent::SelfCritiqueCompleted { .. })),
        Some(HarnessEvent::SelfCritiqueCompleted {
            produced_tool_calls: false,
            ..
        })
    ));
    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("Reviewed: tests pass, change is minimal. Confirmed.")
    );
    // mutate → done(verify pass) → reflection.
    assert_eq!(model.requests().len(), 3);
}

/// Toggle: `self_critique = false` ⇒ no reflection step (just verify).
#[tokio::test]
async fn self_critique_off_skips_reflection() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("fixed"), "ok\n").expect("seed sentinel");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![create_file_call("c1", "feature.txt", "x\n")],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(editing_and_command_tools())
        .verify_config(verify_only("test -f fixed"))
        .max_iterations(10)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("no-critique"),
            UserMessage::new("Add feature."),
        )
        .await
        .expect("run turn");

    let critique = outcome
        .events
        .iter()
        .filter(|event| matches!(event, HarnessEvent::SelfCritiqueCompleted { .. }))
        .count();
    assert_eq!(critique, 0, "no reflection step when self_critique is off");
    assert_eq!(verifications(&outcome.events, true), 1);
    assert_eq!(outcome.assistant_message.as_deref(), Some("Done."));
    assert_eq!(model.requests().len(), 2);
}
