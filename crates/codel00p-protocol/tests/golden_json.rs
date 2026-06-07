use codel00p_protocol::{
    AgentEvent, CompactionRecord, ContextWindowState, EventId, MemoryEntry, MemoryKind,
    MemorySource, MemoryStatus, PermissionDecision, PermissionMode, PermissionRequest,
    PermissionScope, ProjectRef, RuntimeErrorKind, SessionId, SessionMessage,
    SessionPersistenceEvent, ToolCall, ToolProgress, ToolResult, TurnId,
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

#[test]
fn runtime_contracts_match_golden_json() {
    let value = serde_json::to_value(json!({
        "error_kind": RuntimeErrorKind::ContextOverflow,
        "permission_request": PermissionRequest::new(
            "permission-1",
            SessionId::from_static("session-1"),
            TurnId::from_static("turn-1"),
            "write_file",
            json!({ "path": "src/lib.rs" }),
            PermissionScope::WorkspaceWrite,
        ).with_reason("tool wants to modify the workspace"),
        "permission_decision": PermissionDecision::deny(
            "permission-1",
            PermissionMode::Ask,
            "write access has not been approved",
        ),
        "context_window": ContextWindowState::new("gpt-4.1", 200000, 181000)
            .with_warning_threshold(170000)
            .with_blocking_threshold(197000),
        "compaction": CompactionRecord::new(
            EventId::from_static("event-compact"),
            SessionId::from_static("session-1"),
            TurnId::from_static("turn-1"),
            42,
            12,
        ).with_summary("Preserved active task, relevant files, and pending work."),
        "tool_progress": ToolProgress::new(
            EventId::from_static("event-progress"),
            SessionId::from_static("session-1"),
            TurnId::from_static("turn-1"),
            "search_text",
            "started",
        ).with_message("searching workspace"),
        "session_persistence": SessionPersistenceEvent::record_appended(
            EventId::from_static("event-record"),
            SessionId::from_static("session-1"),
            "record-1",
            3,
        ),
    }))
    .expect("serialize runtime contracts");

    assert_eq!(value, fixture("runtime_contracts"));
}

fn fixture(name: &str) -> serde_json::Value {
    let path = format!("tests/fixtures/{name}.json");
    let content = std::fs::read_to_string(&path).expect("read golden fixture");
    serde_json::from_str(&content).expect("parse golden fixture")
}
