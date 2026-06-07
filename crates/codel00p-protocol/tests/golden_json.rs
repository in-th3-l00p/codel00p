use codel00p_protocol::{
    AgentEvent, EventId, MemoryEntry, MemoryKind, MemorySource, MemoryStatus, ProjectRef,
    SessionId, SessionMessage, ToolCall, ToolResult, TurnId,
};
use serde_json::json;

#[test]
fn session_messages_match_golden_json() {
    let value = serde_json::to_value(vec![
        SessionMessage::user("Inspect the repository."),
        SessionMessage::assistant("I will read the README."),
        SessionMessage::tool("call-1", "read_file", json!({ "content": "Agent Harness" })),
    ])
    .expect("serialize session messages");

    assert_eq!(value, fixture("session_messages"));
}

#[test]
fn agent_events_match_golden_json() {
    let value = serde_json::to_value(AgentEvent::tool_call_completed(
        EventId::from_static("event-tool"),
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
        "read_file",
    ))
    .expect("serialize agent event");

    assert_eq!(value, fixture("agent_event_tool_call_completed"));
}

#[test]
fn tool_contracts_match_golden_json() {
    let value = serde_json::to_value(json!({
        "call": ToolCall::new("call-1", "read_file", json!({ "path": "README.md" })),
        "result": ToolResult::success("call-1", "read_file", json!({ "content": "hello" })),
    }))
    .expect("serialize tool contracts");

    assert_eq!(value, fixture("tool_contracts"));
}

#[test]
fn memory_entry_matches_golden_json() {
    let value = serde_json::to_value(
        MemoryEntry::new(
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
        .with_tag("harness"),
    )
    .expect("serialize memory entry");

    assert_eq!(value, fixture("memory_entry"));
}

fn fixture(name: &str) -> serde_json::Value {
    let path = format!("tests/fixtures/{name}.json");
    let content = std::fs::read_to_string(&path).expect("read golden fixture");
    serde_json::from_str(&content).expect("parse golden fixture")
}
