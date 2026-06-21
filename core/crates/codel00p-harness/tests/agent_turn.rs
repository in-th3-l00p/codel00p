mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_harness::{
    AgentEventSink, AgentHarness, ExplicitTurnMemoryExtractor, HarnessError, HarnessEvent,
    HarnessInferenceRequest, HarnessInferenceResponse, MemoryRepositoryCandidateSink,
    MemoryRepositoryProjectMemoryProvider, ModelClient, SessionId, SessionMessage, SessionState,
    TokenSink, ToolRegistry, TurnMemoryExtractionRequest, TurnMemoryExtractor, UserMessage,
    Workspace,
};
use codel00p_memory::{
    InMemoryMemoryStore, MemoryCandidateInput, MemoryListFilter, MemoryRepository, ReviewDecision,
};
use codel00p_protocol::{MemoryKind, MemorySource, MemoryStatus, ProjectRef, TurnId};
use support::ScriptedModelClient;
use tempfile::tempdir;

/// A recorded tool-call delta: (index, id, name, args_fragment).
type RecordedDelta = (usize, Option<String>, Option<String>, String);

/// A model client that always asks for the same read-only tool call, so the run
/// loop never reaches a final assistant message and exhausts its iteration
/// budget — used to observe the *default* iteration ceiling.
#[derive(Clone)]
struct AlwaysToolCallModel;

#[async_trait]
impl ModelClient for AlwaysToolCallModel {
    async fn infer(
        &self,
        _request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        Ok(HarnessInferenceResponse::with_tool_calls(
            "github",
            "gpt-4o",
            vec![codel00p_harness::ModelToolCall::new(
                "call-loop",
                "read_file",
                serde_json::json!({ "path": "README.md" }),
            )],
        ))
    }
}

#[tokio::test]
async fn harness_without_explicit_max_uses_the_default_iteration_ceiling() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");

    // No `.max_iterations(...)` call: the build must fall back to the default.
    let error = AgentHarness::builder()
        .model_client(AlwaysToolCallModel)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-default-iters"),
            UserMessage::new("Loop forever."),
        )
        .await
        .expect_err("loop should hit the iteration limit");

    assert!(
        matches!(error, HarnessError::IterationLimit { limit } if limit == 25),
        "expected the default iteration ceiling of 25, got: {error:?}"
    );
}

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
        &[
            "find_files",
            "grep",
            "list_files",
            "read_file",
            "repo_map",
            "search_text"
        ]
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
async fn event_sink_receives_events_in_outcome_order() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);
    let sink = RecordingEventSink::default();

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .event_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-event-sink"),
            UserMessage::new("Run."),
        )
        .await
        .expect("run turn");

    assert_eq!(sink.events(), outcome.events);
}

#[tokio::test]
async fn event_sink_receives_early_events_before_turn_error() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(Vec::new());
    let sink = RecordingEventSink::default();

    let error = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .event_sink(sink.clone())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-event-sink-error"),
            UserMessage::new("Run."),
        )
        .await
        .expect_err("scripted model should fail");

    assert!(
        error
            .to_string()
            .contains("scripted model client has no response")
    );
    let events = sink.events();
    assert!(matches!(
        events.first(),
        Some(HarnessEvent::TurnStarted { .. })
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, HarnessEvent::ContextBuilt { .. }))
    );
}

#[tokio::test]
async fn run_turn_with_state_preserves_prior_messages_before_new_user_message() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Continue from the previous answer.",
    )]);
    let mut session_state = SessionState::new(SessionId::from_static("session-resume"));
    session_state.push_user(UserMessage::new("Initial request."));
    session_state.push_assistant("Initial answer.");

    let outcome = AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .build()
        .expect("build harness")
        .run_turn_with_state(
            session_state,
            UserMessage::new("Continue with the next step."),
        )
        .await
        .expect("run resumed turn");

    assert_eq!(
        model.requests()[0].session_state().messages(),
        &[
            SessionMessage::user("Initial request."),
            SessionMessage::assistant("Initial answer."),
            SessionMessage::user("Continue with the next step."),
        ]
    );
    assert_eq!(
        outcome.session_state.messages(),
        &[
            SessionMessage::user("Initial request."),
            SessionMessage::assistant("Initial answer."),
            SessionMessage::user("Continue with the next step."),
            SessionMessage::assistant("Continue from the previous answer."),
        ]
    );
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
            // Proactive recall disabled: this case asserts the static filter-only
            // retrieval path (configured kind/tag/text + the legacy reason string).
            MemoryRepositoryProjectMemoryProvider::new(project, store)
                .with_proactive(false)
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

/// Seeds two approved memories so a task-aware query can pick the relevant one.
fn seed_recall_memories(project: &ProjectRef) -> InMemoryMemoryStore {
    let source = MemorySource::turn(
        SessionId::from_static("recall-source"),
        TurnId::from_static("recall-turn"),
    );
    let mut store = InMemoryMemoryStore::default();
    for (id, content) in [
        (
            "mem-deploy",
            "Deploy the service to the kubernetes cluster.",
        ),
        ("mem-style", "Indent with four spaces, never tabs."),
    ] {
        store
            .create_candidate(MemoryCandidateInput::new(
                id,
                project.clone(),
                MemoryKind::Workflow,
                content,
                source.clone(),
            ))
            .expect("create candidate");
        store
            .review(id, ReviewDecision::approve("alice"))
            .expect("approve candidate");
    }
    store
}

#[tokio::test]
async fn run_turn_proactively_recalls_memory_matching_the_task() {
    // Proactive recall is ON by default: the provider must USE the turn's task
    // text (previously the request was ignored) to surface the relevant memory.
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);
    let project = ProjectRef::new("project-1", "codel00p");
    let store = seed_recall_memories(&project);

    AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        // No static text configured — recall is driven purely by the task.
        .project_memory_provider(MemoryRepositoryProjectMemoryProvider::new(project, store))
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-recall"),
            UserMessage::new("How do I deploy to kubernetes?"),
        )
        .await
        .expect("run turn");

    let requests = model.requests();
    let memory = requests[0].project_memory().expect("project memory");
    assert_eq!(memory.items().len(), 1);
    assert_eq!(memory.items()[0].id(), "mem-deploy");
}

#[tokio::test]
async fn run_turn_ignores_task_when_proactive_recall_disabled() {
    // With proactive recall OFF, the task text is ignored: retrieval falls back
    // to the static filter-only path, returning every approved memory (no text
    // filter configured) regardless of the task.
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github", "gpt-4o", "Done.",
    )]);
    let project = ProjectRef::new("project-1", "codel00p");
    let store = seed_recall_memories(&project);

    AgentHarness::builder()
        .model_client(model.clone())
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .project_memory_provider(
            MemoryRepositoryProjectMemoryProvider::new(project, store).with_proactive(false),
        )
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("session-no-recall"),
            UserMessage::new("How do I deploy to kubernetes?"),
        )
        .await
        .expect("run turn");

    let requests = model.requests();
    let memory = requests[0].project_memory().expect("project memory");
    // Both memories returned (task ignored), sorted by id.
    let ids: Vec<&str> = memory.items().iter().map(|item| item.id()).collect();
    assert_eq!(ids, ["mem-deploy", "mem-style"]);
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
                "mem-duplicate-new",
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

#[derive(Clone, Default)]
struct RecordingEventSink {
    events: Arc<Mutex<Vec<HarnessEvent>>>,
}

impl RecordingEventSink {
    fn events(&self) -> Vec<HarnessEvent> {
        self.events.lock().expect("events").clone()
    }
}

#[async_trait]
impl AgentEventSink for RecordingEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        self.events.lock().expect("events").push(event.clone());
    }
}

#[tokio::test]
async fn run_turn_streams_assistant_text_to_the_token_sink() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let model = ScriptedModelClient::new(vec![HarnessInferenceResponse::assistant(
        "github",
        "gpt-4o",
        "Streamed answer.",
    )]);
    let session_id = SessionId::from_static("session-stream");

    let collected: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let sink_collected = collected.clone();
    let sink = move |token: &str| sink_collected.lock().unwrap().push_str(token);

    let outcome = AgentHarness::builder()
        .model_client(model)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .token_sink(sink)
        .build()
        .expect("build harness")
        .run_turn(session_id, UserMessage::new("Answer please."))
        .await
        .expect("run turn");

    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("Streamed answer.")
    );
    assert_eq!(collected.lock().unwrap().as_str(), "Streamed answer.");
}

/// A model client whose streaming path emits a sequence of tool-call argument
/// deltas to the sink before returning a (plain) assistant message, mimicking a
/// provider that streams a tool call being assembled.
#[derive(Clone)]
struct StreamingDeltaModelClient;

#[async_trait]
impl ModelClient for StreamingDeltaModelClient {
    async fn infer(
        &self,
        _request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        Ok(HarnessInferenceResponse::assistant(
            "github", "gpt-4o", "Done.",
        ))
    }

    async fn infer_streaming(
        &self,
        request: HarnessInferenceRequest,
        sink: &dyn TokenSink,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        sink.on_tool_call_delta(0, Some("call_1"), Some("read_file"), "");
        sink.on_tool_call_delta(0, Some("call_1"), None, "{\"path\"");
        sink.on_tool_call_delta(0, Some("call_1"), None, ":\"a.txt\"}");
        self.infer(request).await
    }
}

#[tokio::test]
async fn run_turn_surfaces_tool_call_deltas_to_token_sink_and_event_stream() {
    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let session_id = SessionId::from_static("session-tool-delta");

    // Token sink records every tool-call delta fragment it receives.
    #[derive(Clone, Default)]
    struct DeltaTokenSink {
        deltas: Arc<Mutex<Vec<RecordedDelta>>>,
    }
    impl TokenSink for DeltaTokenSink {
        fn on_token(&self, _token: &str) {}
        fn on_tool_call_delta(
            &self,
            index: usize,
            id: Option<&str>,
            name: Option<&str>,
            args_fragment: &str,
        ) {
            self.deltas.lock().unwrap().push((
                index,
                id.map(str::to_string),
                name.map(str::to_string),
                args_fragment.to_string(),
            ));
        }
    }

    let token_sink = DeltaTokenSink::default();
    let recorded = token_sink.deltas.clone();
    let event_sink = RecordingEventSink::default();

    let outcome = AgentHarness::builder()
        .model_client(StreamingDeltaModelClient)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .token_sink(token_sink)
        .event_sink(event_sink.clone())
        .build()
        .expect("build harness")
        .run_turn(session_id, UserMessage::new("Read a.txt."))
        .await
        .expect("run turn");

    assert_eq!(outcome.assistant_message.as_deref(), Some("Done."));

    // The token sink saw the fragments incrementally and in order.
    let deltas = recorded.lock().unwrap();
    assert_eq!(
        *deltas,
        vec![
            (
                0,
                Some("call_1".to_string()),
                Some("read_file".to_string()),
                String::new()
            ),
            (0, Some("call_1".to_string()), None, "{\"path\"".to_string()),
            (
                0,
                Some("call_1".to_string()),
                None,
                ":\"a.txt\"}".to_string()
            ),
        ]
    );

    // The event stream carried matching `ToolCallArgsDelta` events.
    let args_deltas: Vec<RecordedDelta> = event_sink
        .events()
        .into_iter()
        .filter_map(|event| match event {
            HarnessEvent::ToolCallArgsDelta {
                index,
                id,
                name,
                args_fragment,
                ..
            } => Some((index, id, name, args_fragment)),
            _ => None,
        })
        .collect();
    assert_eq!(
        args_deltas,
        vec![
            (
                0,
                Some("call_1".to_string()),
                Some("read_file".to_string()),
                String::new()
            ),
            (0, Some("call_1".to_string()), None, "{\"path\"".to_string()),
            (
                0,
                Some("call_1".to_string()),
                None,
                ":\"a.txt\"}".to_string()
            ),
        ]
    );
}
