use codel00p_harness::{
    ContextWindowState, HarnessEvent, ModelToolCall, PermissionDecision, PermissionMode,
    PermissionRequest, PermissionScope, RuntimeErrorKind, SessionId, SessionMessage, SessionState,
};
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

#[test]
fn harness_reexports_runtime_protocol_contracts() {
    let request = PermissionRequest::new(
        "permission-protocol",
        SessionId::from_static("session-protocol"),
        TurnId::from_static("turn-protocol"),
        "read_file",
        json!({ "path": "README.md" }),
        PermissionScope::ReadOnly,
    );
    let decision = PermissionDecision::allow(request.id(), PermissionMode::Allow);
    let window = ContextWindowState::new("gpt-4o", 128_000, 12_000);
    let error_kind = RuntimeErrorKind::PermissionDenied;

    assert!(decision.allows_execution());
    assert_eq!(window.context_limit_tokens(), 128_000);
    assert_eq!(
        serde_json::to_value(error_kind).expect("serialize kind"),
        json!("permission_denied")
    );
}
