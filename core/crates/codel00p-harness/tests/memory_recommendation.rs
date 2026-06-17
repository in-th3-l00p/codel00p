//! End-to-end tests for post-session memory recommendations (Memory 2.0).
//!
//! These mirror the explicit-memory tests in `agent_turn.rs` and the
//! capability auto-extraction tests in `capability_turn.rs`, but exercise the
//! *automatic* recommender: after a productive, mutating turn the
//! `DeterministicMemoryRecommender` proposes 0..N memory candidates into the
//! same review/candidate queue that explicit `remember:` directives flow into —
//! with no auto-approval. All tests here are offline and deterministic.

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, DeterministicMemoryRecommender, HarnessError, HarnessEvent,
    HarnessInferenceResponse, MemoryCandidateSink, MemoryCandidateSinkOutcome, MemoryRecommender,
    ModelToolCall, SessionId, ToolRegistry, TurnMemoryRecommendationRequest, UserMessage,
    Workspace,
};
use codel00p_memory::MemoryCandidateInput;
use codel00p_protocol::ProjectRef;
use serde_json::json;
use support::ScriptedModelClient;
use tempfile::tempdir;

/// Records every candidate persisted, for assertions.
#[derive(Clone, Default)]
struct RecordingSink {
    candidates: Arc<Mutex<Vec<MemoryCandidateInput>>>,
}

#[async_trait]
impl MemoryCandidateSink for RecordingSink {
    async fn persist(
        &self,
        candidates: Vec<MemoryCandidateInput>,
    ) -> Result<MemoryCandidateSinkOutcome, HarnessError> {
        let ids: Vec<String> = candidates.iter().map(|c| c.id().to_string()).collect();
        self.candidates.lock().unwrap().extend(candidates);
        Ok(MemoryCandidateSinkOutcome::from_parts(ids, Vec::new()))
    }
}

/// A sink that always fails, to prove a failing recommendation path is
/// non-fatal and recorded as a lifecycle-hook failure.
#[derive(Clone, Default)]
struct FailingRecommender;

#[async_trait]
impl MemoryRecommender for FailingRecommender {
    async fn recommend(
        &self,
        _request: TurnMemoryRecommendationRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError> {
        Err(HarnessError::InferenceFailed {
            message: "recommender exploded".to_string(),
        })
    }
}

fn project() -> ProjectRef {
    ProjectRef::new("project-1", "codel00p")
}

#[tokio::test]
async fn productive_mutating_turn_recommends_a_candidate() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    // The model runs two mutating tools, then answers — a "productive" turn.
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![
                ModelToolCall::new(
                    "call-1",
                    "create_file",
                    json!({ "path": "src/widget.rs", "content": "// widget\n" }),
                ),
                ModelToolCall::new(
                    "call-2",
                    "create_file",
                    json!({ "path": "tests/widget_test.rs", "content": "// test\n" }),
                ),
            ],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Scaffolded the widget module."),
    ]);

    let sink = RecordingSink::default();
    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .memory_recommender(Arc::new(
            DeterministicMemoryRecommender::new(project()).with_min_mutations(2),
        ))
        .memory_candidate_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-rec"),
            UserMessage::new("Scaffold the widget module"),
        )
        .await
        .expect("run turn");

    let candidates = sink.candidates.lock().unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "expected one auto-recommended candidate"
    );
    let candidate = &candidates[0];

    // Deterministic id scheme mirrors the explicit extractor.
    assert_eq!(
        candidate.id(),
        "memory-candidate-session-rec-{turn}-1"
            .replace("{turn}", candidate.source().turn_id().as_str())
    );
    // Source tracks the originating turn.
    assert_eq!(candidate.source().session_id().as_str(), "session-rec");
    // Tagged so reviewers can see it was machine-proposed.
    assert!(candidate.tags().iter().any(|t| t == "auto-recommended"));
    // Content references the goal and the mutating work.
    assert!(candidate.content().contains("widget"));

    // Turn still completes normally.
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { .. })
    ));
}

#[tokio::test]
async fn read_only_turn_recommends_nothing() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    // No tool calls at all — a trivial, read-only turn.
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Here is the answer to your question.",
    )]);

    let sink = RecordingSink::default();
    AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .memory_recommender(Arc::new(DeterministicMemoryRecommender::new(project())))
        .memory_candidate_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-readonly"),
            UserMessage::new("What does this function do?"),
        )
        .await
        .expect("run turn");

    assert!(
        sink.candidates.lock().unwrap().is_empty(),
        "a read-only turn should recommend nothing"
    );
}

#[tokio::test]
async fn failing_recommender_records_hook_failure_and_completes() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "create_file",
                json!({ "path": "src/a.rs", "content": "// a\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    let sink = RecordingSink::default();
    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .memory_recommender(Arc::new(FailingRecommender))
        .memory_candidate_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-fail"),
            UserMessage::new("Create a file"),
        )
        .await
        .expect("run turn should still succeed");

    // The failure surfaced as a non-fatal lifecycle-hook failure...
    assert!(
        outcome.events.iter().any(|event| matches!(
            event,
            HarnessEvent::LifecycleHookFailed { hook, .. } if hook == "memory_recommendation"
        )),
        "expected a memory_recommendation LifecycleHookFailed event"
    );
    // ...and the turn still completed and persisted nothing.
    assert!(sink.candidates.lock().unwrap().is_empty());
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { .. })
    ));
}

#[tokio::test]
async fn no_recommender_configured_is_backward_compatible() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![
        HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![ModelToolCall::new(
                "call-1",
                "create_file",
                json!({ "path": "src/a.rs", "content": "// a\n" }),
            )],
        ),
        HarnessInferenceResponse::assistant("github", "gpt-4o", "Done."),
    ]);

    // A sink is wired but NO recommender — recommendation must be a no-op.
    let sink = RecordingSink::default();
    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::editing_defaults())
        .memory_candidate_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-none"),
            UserMessage::new("Create a file"),
        )
        .await
        .expect("run turn");

    assert!(sink.candidates.lock().unwrap().is_empty());
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { .. })
    ));
}

/// The recommender itself, exercised directly (offline), to pin its
/// deterministic behavior independent of the harness wiring.
#[tokio::test]
async fn recommender_is_deterministic_and_skips_trivial_turns() {
    let recommender = DeterministicMemoryRecommender::new(project());

    // A mutating, finished turn yields a stable candidate.
    let productive = TurnMemoryRecommendationRequest::new(
        SessionId::from_static("s"),
        codel00p_protocol::TurnId::from_static("t"),
        "Add a retry helper".to_string(),
        Some("Added it.".to_string()),
        vec![
            ("apply_patch".to_string(), None),
            ("git_commit".to_string(), None),
        ],
    );
    let first = recommender.recommend(productive.clone()).await.unwrap();
    let second = recommender.recommend(productive).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id(), "memory-candidate-s-t-1");
    assert_eq!(first, second, "recommendation must be deterministic");

    // No assistant message → unfinished → nothing.
    let unfinished = TurnMemoryRecommendationRequest::new(
        SessionId::from_static("s"),
        codel00p_protocol::TurnId::from_static("t"),
        "x".to_string(),
        None,
        vec![("apply_patch".to_string(), None)],
    );
    assert!(recommender.recommend(unfinished).await.unwrap().is_empty());

    // Only read-only tools → nothing.
    let readonly = TurnMemoryRecommendationRequest::new(
        SessionId::from_static("s"),
        codel00p_protocol::TurnId::from_static("t"),
        "x".to_string(),
        Some("done".to_string()),
        vec![("read_file".to_string(), None), ("grep".to_string(), None)],
    );
    assert!(recommender.recommend(readonly).await.unwrap().is_empty());
}
