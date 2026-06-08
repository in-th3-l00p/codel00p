mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    AgentHarness, ExplicitTurnMemoryExtractor, HarnessError, HarnessEvent,
    HarnessInferenceResponse, MemoryRepositoryCandidateSink, MemoryRepositoryProjectMemoryProvider,
    SessionId, SessionMessage, ToolRegistry, TurnMemoryExtractionRequest, TurnMemoryExtractor,
    UserMessage, Workspace,
};
use codel00p_memory::{
    InMemoryMemoryStore, MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, TurnId};
use support::ScriptedModelClient;
use tempfile::tempdir;

#[tokio::test]
async fn run_turn_returns_final_assistant_message_without_tools() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "This is a Rust workspace.",
    )]);
    let session_id = SessionId::from_static("session-agent");

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            session_id.clone(),
            UserMessage::new("Summarize this project."),
        )
        .await
        .expect("run turn");

    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("This is a Rust workspace.")
    );
    assert_eq!(
        outcome.session_state.messages(),
        &[
            SessionMessage::user("Summarize this project."),
            SessionMessage::assistant("This is a Rust workspace."),
        ]
    );
    assert_eq!(model.requests().len(), 1);
    assert_eq!(
        model.requests()[0].tool_names(),
        &["list_files", "read_file", "search_text"]
    );
    assert_eq!(
        model.requests()[0].workspace_root(),
        Some(
            dir.path()
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
                .as_str()
        )
    );

    assert!(matches!(
        outcome.events.first(),
        Some(HarnessEvent::TurnStarted { session_id: id, .. }) if *id == session_id
    ));
    assert!(outcome.events.iter().any(|event| matches!(
        event,
        HarnessEvent::ContextBuilt {
            message_count: 1,
            ..
        }
    )));
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::InferenceRequested { provider, model, .. }
                if provider == "github" && model == "gpt-4o"
        )
    }));
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { iterations: 1, .. })
    ));
}

#[tokio::test]
async fn run_turn_sends_approved_project_memory_to_model_client() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Use the harness memory.",
    )]);
    let project = ProjectRef::new("project-1", "codel00p");
    let source = MemorySource::turn(
        SessionId::from_static("session-memory-source"),
        TurnId::from_static("turn-memory-source"),
    );
    let mut store = InMemoryMemoryStore::default();
    store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-harness",
                project.clone(),
                MemoryKind::Architecture,
                "The harness owns tool execution.",
                source.clone(),
            )
            .with_tag("harness"),
        )
        .expect("create harness memory");
    store
        .review("mem-harness", ReviewDecision::approve("alice"))
        .expect("approve harness memory");
    store
        .create_candidate(MemoryCandidateInput::new(
            "mem-workflow",
            project.clone(),
            MemoryKind::Workflow,
            "Run pnpm verify before pushing.",
            source,
        ))
        .expect("create workflow candidate");

    AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .project_memory_provider(
            MemoryRepositoryProjectMemoryProvider::new(project, store)
                .with_kind(MemoryKind::Architecture)
                .with_tag("harness")
                .with_text("tool execution")
                .with_limit(4),
        )
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-memory"),
            UserMessage::new("Summarize this project."),
        )
        .await
        .expect("run turn");

    let requests = model.requests();
    let memory = requests[0].project_memory().expect("project memory");

    assert_eq!(memory.items().len(), 1);
    assert_eq!(memory.items()[0].id(), "mem-harness");
    assert_eq!(memory.items()[0].kind(), MemoryKind::Architecture);
    assert_eq!(
        memory.items()[0].content(),
        "The harness owns tool execution."
    );
    assert_eq!(memory.items()[0].tags(), ["harness"]);
    assert_eq!(
        memory.items()[0].reason(),
        "matched kind architecture and tag harness and text tool execution"
    );
}

#[tokio::test]
async fn run_turn_extracts_explicit_memory_candidates_after_completion() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "remember workflow[verify]: Run pnpm verify before pushing main.",
    )]);
    let project = ProjectRef::new("project-1", "codel00p");
    let store = Arc::new(Mutex::new(InMemoryMemoryStore::default()));

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .turn_memory_extractor(
            ExplicitTurnMemoryExtractor::new(project.clone()).with_tag("assistant"),
        )
        .memory_candidate_sink(MemoryRepositoryCandidateSink::new_shared(store.clone()))
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-extract"),
            UserMessage::new("Summarize what to remember."),
        )
        .await
        .expect("run turn");

    let listed = store
        .lock()
        .expect("store lock")
        .list(MemoryListFilter::new(project).with_status(MemoryStatus::Candidate))
        .expect("list candidate memory");

    assert_eq!(listed.len(), 1);
    assert!(
        listed[0]
            .entry()
            .id()
            .starts_with("memory-candidate-session-extract-turn-")
    );
    assert!(listed[0].entry().id().ends_with("-1"));
    assert_eq!(listed[0].entry().status(), MemoryStatus::Candidate);
    assert_eq!(listed[0].entry().kind(), MemoryKind::Workflow);
    assert_eq!(
        listed[0].entry().content(),
        "Run pnpm verify before pushing main."
    );
    assert_eq!(listed[0].entry().tags(), ["assistant", "verify"]);
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { .. })
    ));
}

#[tokio::test]
async fn run_turn_without_memory_directives_persists_no_candidates() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "There is nothing durable to remember here.",
    )]);
    let project = ProjectRef::new("project-1", "codel00p");
    let store = Arc::new(Mutex::new(InMemoryMemoryStore::default()));

    AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .turn_memory_extractor(ExplicitTurnMemoryExtractor::new(project.clone()))
        .memory_candidate_sink(MemoryRepositoryCandidateSink::new_shared(store.clone()))
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-no-memory"),
            UserMessage::new("Answer normally."),
        )
        .await
        .expect("run turn");

    let listed = store
        .lock()
        .expect("store lock")
        .list(MemoryListFilter::new(project))
        .expect("list memory");

    assert!(listed.is_empty());
}

#[tokio::test]
async fn extraction_failures_are_nonfatal_and_recorded_as_lifecycle_failures() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "remember workflow[verify]: Run pnpm verify before pushing main.",
    )]);
    let store = Arc::new(Mutex::new(InMemoryMemoryStore::default()));

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .turn_memory_extractor(FailingTurnMemoryExtractor)
        .memory_candidate_sink(MemoryRepositoryCandidateSink::new_shared(store.clone()))
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-extraction-fails"),
            UserMessage::new("Summarize what to remember."),
        )
        .await
        .expect("run turn");

    assert!(
        store
            .lock()
            .expect("store lock")
            .list(MemoryListFilter::new(ProjectRef::new(
                "project-1",
                "codel00p"
            )))
            .expect("list memory")
            .is_empty()
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::LifecycleHookFailed { hook, message, .. }
                if hook == "memory_extraction" && message.contains("extract failed")
        )
    }));
}

#[tokio::test]
async fn duplicate_extracted_candidates_are_skipped_without_auto_approval() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "remember workflow[verify]: Run pnpm verify before pushing main.",
    )]);
    let project = ProjectRef::new("project-1", "codel00p");
    let source = MemorySource::turn(
        SessionId::from_static("session-duplicate"),
        TurnId::from_static("turn-duplicate"),
    );
    let mut initial_store = InMemoryMemoryStore::default();
    initial_store
        .create_candidate(
            MemoryCandidateInput::new(
                "mem-duplicate",
                project.clone(),
                MemoryKind::Workflow,
                "Run pnpm verify before pushing main.",
                source.clone(),
            )
            .with_tag("verify"),
        )
        .expect("seed candidate");
    let store = Arc::new(Mutex::new(initial_store));

    AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .turn_memory_extractor(FixedCandidateExtractor {
            candidate: MemoryCandidateInput::new(
                "mem-duplicate",
                project.clone(),
                MemoryKind::Workflow,
                "Run pnpm verify before pushing main.",
                source,
            )
            .with_tag("verify"),
        })
        .memory_candidate_sink(MemoryRepositoryCandidateSink::new_shared(store.clone()))
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-duplicate"),
            UserMessage::new("Summarize what to remember."),
        )
        .await
        .expect("run turn");

    let listed = store
        .lock()
        .expect("store lock")
        .list(MemoryListFilter::new(project))
        .expect("list memory");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entry().id(), "mem-duplicate");
    assert_eq!(listed[0].entry().status(), MemoryStatus::Candidate);
}

struct FailingTurnMemoryExtractor;

#[async_trait]
impl TurnMemoryExtractor for FailingTurnMemoryExtractor {
    async fn extract(
        &self,
        _request: TurnMemoryExtractionRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError> {
        Err(HarnessError::Configuration {
            message: "extract failed".to_string(),
        })
    }
}

struct FixedCandidateExtractor {
    candidate: MemoryCandidateInput,
}

#[async_trait]
impl TurnMemoryExtractor for FixedCandidateExtractor {
    async fn extract(
        &self,
        _request: TurnMemoryExtractionRequest,
    ) -> Result<Vec<MemoryCandidateInput>, HarnessError> {
        Ok(vec![self.candidate.clone()])
    }
}
