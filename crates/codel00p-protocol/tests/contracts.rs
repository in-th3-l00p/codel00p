use codel00p_protocol::{
    AgentEvent, EventId, MemoryEntry, MemoryKind, MemorySource, MemoryStatus, ProjectRef,
    ProtocolVersion, ProviderRef, SessionId, SessionMessage, SessionRole, ToolCall, ToolResult,
    TurnId,
};
use serde_json::json;

#[test]
fn protocol_version_is_serializable_and_stable() {
    let version = ProtocolVersion::current();

    assert_eq!(version.as_str(), "codel00p.protocol.v1");
    assert_eq!(
        serde_json::to_value(version).expect("serialize version"),
        json!("codel00p.protocol.v1")
    );
}

#[test]
fn ids_are_string_newtypes_with_test_constructors() {
    let session_id = SessionId::from_static("session-fixed");
    let turn_id = TurnId::from_static("turn-fixed");
    let event_id = EventId::new();

    assert_eq!(session_id.as_str(), "session-fixed");
    assert_eq!(turn_id.as_str(), "turn-fixed");
    assert!(event_id.as_str().starts_with("event-"));
}

#[test]
fn session_messages_round_trip_as_protocol_json() {
    let messages = vec![
        SessionMessage::user("Inspect the repository."),
        SessionMessage::assistant("I will read the README."),
        SessionMessage::tool("call-1", "read_file", json!({ "content": "Agent Harness" })),
    ];

    let encoded = serde_json::to_string(&messages).expect("serialize messages");
    let decoded: Vec<SessionMessage> =
        serde_json::from_str(&encoded).expect("deserialize messages");

    assert_eq!(decoded, messages);
    assert_eq!(decoded[0].role(), SessionRole::User);
}

#[test]
fn tool_call_and_result_keep_structured_json_payloads() {
    let call = ToolCall::new("call-1", "read_file", json!({ "path": "README.md" }));
    let result = ToolResult::success("call-1", "read_file", json!({ "content": "hello" }));

    assert_eq!(call.id(), "call-1");
    assert_eq!(call.name(), "read_file");
    assert_eq!(call.input()["path"], "README.md");
    assert_eq!(result.output()["content"], "hello");
}

#[test]
fn agent_events_capture_runtime_boundaries() {
    let event = AgentEvent::tool_call_completed(
        EventId::from_static("event-tool"),
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
        "read_file",
    );

    let encoded = serde_json::to_value(&event).expect("serialize event");

    assert_eq!(encoded["kind"], "tool_call_completed");
    assert_eq!(encoded["session_id"], "session-1");
    assert_eq!(encoded["turn_id"], "turn-1");
    assert_eq!(encoded["tool_name"], "read_file");
}

#[test]
fn memory_entries_capture_reviewed_project_knowledge() {
    let memory = MemoryEntry::new(
        "mem-1",
        ProjectRef::new("project-1", "codel00p"),
        MemoryKind::Architecture,
        "The harness owns tool execution; providers own inference.",
    )
    .with_status(MemoryStatus::Approved)
    .with_source(MemorySource::turn(
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
    ))
    .with_tag("harness");

    assert_eq!(memory.id(), "mem-1");
    assert_eq!(memory.project().name(), "codel00p");
    assert_eq!(memory.status(), MemoryStatus::Approved);
    assert_eq!(memory.tags(), &["harness"]);
}

#[test]
fn provider_refs_keep_routing_identity_separate_from_memory() {
    let provider = ProviderRef::new("github", "gpt-4o-mini-2024-07-18")
        .with_alias("copilot")
        .with_base_url("https://api.githubcopilot.com");

    assert_eq!(provider.provider(), "github");
    assert_eq!(provider.model(), "gpt-4o-mini-2024-07-18");
    assert_eq!(provider.alias(), Some("copilot"));
    assert_eq!(provider.base_url(), Some("https://api.githubcopilot.com"));
}
