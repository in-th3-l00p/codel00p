use codel00p_harness::{HarnessEvent, ModelToolCall, SessionId, SessionMessage, SessionState};
use codel00p_protocol::{
    AgentEvent, EventId, SessionId as ProtocolSessionId, SessionMessage as ProtocolSessionMessage,
    ToolCall as ProtocolToolCall, TurnId,
};
use serde_json::json;

#[test]
fn harness_reexports_protocol_session_contracts() {
    let session_id: ProtocolSessionId = SessionId::from_static("session-protocol");
    let message: ProtocolSessionMessage = SessionMessage::user("Use shared contracts.");
    let state = SessionState::new(session_id.clone());

    assert_eq!(state.session_id(), &session_id);
    assert_eq!(message.content(), "Use shared contracts.");
}

#[test]
fn harness_events_are_protocol_agent_events() {
    let event: AgentEvent = HarnessEvent::tool_call_completed(
        EventId::from_static("event-protocol"),
        SessionId::from_static("session-protocol"),
        TurnId::from_static("turn-protocol"),
        "read_file",
    );

    let value = serde_json::to_value(event).expect("serialize event");

    assert_eq!(value["kind"], "tool_call_completed");
    assert_eq!(value["event_id"], "event-protocol");
}

#[test]
fn harness_model_tool_calls_are_protocol_tool_calls() {
    let call: ProtocolToolCall =
        ModelToolCall::new("call-protocol", "read_file", json!({ "path": "README.md" }));

    assert_eq!(call.id(), "call-protocol");
    assert_eq!(call.name(), "read_file");
}
