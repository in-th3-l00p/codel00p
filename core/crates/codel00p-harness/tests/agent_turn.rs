mod support;

use codel00p_harness::{
    AgentHarness, HarnessEvent, HarnessInferenceResponse, MemoryRepositoryProjectMemoryProvider,
    SessionId, SessionMessage, ToolRegistry, UserMessage, Workspace,
};
use codel00p_memory::{
    InMemoryMemoryStore, MemoryCandidateInput, MemoryRepository, ReviewDecision,
};
use codel00p_protocol::{MemoryKind, MemorySource, ProjectRef, TurnId};
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
    assert_eq!(memory.items()[0].reason(), "matched kind architecture");
}
