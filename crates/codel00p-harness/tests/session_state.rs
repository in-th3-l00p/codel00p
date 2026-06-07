use codel00p_harness::{
    HarnessEvent, SessionId, SessionMessage, SessionState, TurnId, UserMessage,
};
use codel00p_protocol::EventId;

#[test]
fn session_state_accumulates_messages_in_order() {
    let session_id = SessionId::from_static("session-1");
    let mut state = SessionState::new(session_id.clone());

    state.push_user(UserMessage::new("Inspect the project."));
    state.push_assistant("The project is a Rust workspace.");
    state.push_tool_result("call-1", "read_file", "README.md content");

    assert_eq!(state.session_id(), &session_id);
    assert_eq!(
        state.messages(),
        &[
            SessionMessage::user("Inspect the project."),
            SessionMessage::assistant("The project is a Rust workspace."),
            SessionMessage::tool_result("call-1", "read_file", "README.md content"),
        ]
    );
}

#[test]
fn session_state_serializes_without_losing_messages() {
    let mut state = SessionState::new(SessionId::from_static("session-json"));
    state.push_user(UserMessage::new("Summarize docs."));
    state.push_tool_result(
        "call-json",
        "search_text",
        "docs/harness.md:1:Agent Harness",
    );

    let encoded = serde_json::to_string(&state).expect("session state should serialize");
    let decoded: SessionState =
        serde_json::from_str(&encoded).expect("session state should deserialize");

    assert_eq!(decoded, state);
}

#[test]
fn ids_can_be_generated_or_constructed_for_tests() {
    let generated_session = SessionId::new();
    let generated_turn = TurnId::new();

    assert!(!generated_session.as_str().is_empty());
    assert!(!generated_turn.as_str().is_empty());
    assert_eq!(SessionId::from_static("fixed").as_str(), "fixed");
    assert_eq!(TurnId::from_static("turn-fixed").as_str(), "turn-fixed");
}

#[test]
fn harness_events_are_serializable() {
    let event = HarnessEvent::TurnStarted {
        event_id: EventId::from_static("event-events"),
        session_id: SessionId::from_static("session-events"),
        turn_id: TurnId::from_static("turn-events"),
    };

    let encoded = serde_json::to_string(&event).expect("event should serialize");

    assert!(encoded.contains("turn_started"));
    assert!(encoded.contains("session-events"));
    assert!(encoded.contains("turn-events"));
}
