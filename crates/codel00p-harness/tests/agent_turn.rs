mod support;

use codel00p_harness::{
    AgentHarness, HarnessEvent, HarnessInferenceResponse, SessionId, SessionMessage, ToolRegistry,
    UserMessage, Workspace,
};
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
    assert!(
        outcome
            .events
            .iter()
            .any(|event| matches!(event, HarnessEvent::ContextBuilt { message_count: 1 }))
    );
    assert!(outcome.events.iter().any(|event| {
        matches!(
            event,
            HarnessEvent::InferenceRequested { provider, model }
                if provider == "github" && model == "gpt-4o"
        )
    }));
    assert!(matches!(
        outcome.events.last(),
        Some(HarnessEvent::TurnCompleted { iterations: 1 })
    ));
}
