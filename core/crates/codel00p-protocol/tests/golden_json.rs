use codel00p_protocol::{
    Agent, AgentEvent, CompactionRecord, ContextWindowState, EventId, McpServer, McpTransport,
    MemoryAuditEntry, MemoryEntry, MemoryKind, MemoryReviewAction, MemorySource, MemoryStatus,
    OrgRef, OrgRole, PermissionDecision, PermissionMode, PermissionRequest, PermissionScope,
    Project, ProjectRef, RuntimeErrorKind, SessionId, SessionMessage, SessionPersistenceEvent,
    ToolCall, ToolProgress, ToolResult, TurnId, Viewer,
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

#[test]
fn viewer_matches_golden_json() {
    let value = serde_json::to_value(Viewer::new("user_123").with_email("dev@team.dev").with_org(
        OrgRef::new("org_123", "Acme Engineering").with_slug("acme"),
        OrgRole::Admin,
    ))
    .expect("serialize viewer");

    assert_eq!(value, fixture("viewer"));
}

#[test]
fn project_matches_golden_json() {
    let value = serde_json::to_value(
        Project::new("project-1", "org_123", "codel00p", "codel00p")
            .with_repository_url("https://github.com/in-th3-l00p/codel00p"),
    )
    .expect("serialize project");

    assert_eq!(value, fixture("project"));
}

#[test]
fn memory_audit_entry_matches_golden_json() {
    let value = serde_json::to_value(MemoryAuditEntry::new(
        "mem-1",
        MemoryReviewAction::Approved,
        "user_admin",
    ))
    .expect("serialize audit entry");

    assert_eq!(value, fixture("memory_audit_entry"));
}

#[test]
fn agent_matches_golden_json() {
    let value = serde_json::to_value(
        Agent::new(
            "agent_1",
            "org_123",
            "proj_1",
            "Release reviewer",
            "anthropic",
            "claude-opus-4-8",
            "user_admin",
        )
        .with_description("Reviews release branches.")
        .with_instructions("Be terse. Flag risky changes.")
        .with_mcp_server_ids(vec!["mcp_1".to_string()]),
    )
    .expect("serialize agent");

    assert_eq!(value, fixture("agent"));
}

#[test]
fn mcp_server_matches_golden_json() {
    let value = serde_json::to_value(
        McpServer::new(
            "mcp_1",
            "org_123",
            "proj_1",
            "GitHub",
            McpTransport::Http,
            "user_admin",
        )
        .with_url("https://mcp.github.example/sse"),
    )
    .expect("serialize mcp server");

    assert_eq!(value, fixture("mcp_server"));
}

fn fixture(name: &str) -> serde_json::Value {
    let path = format!("tests/fixtures/{name}.json");
    let content = std::fs::read_to_string(&path).expect("read golden fixture");
    serde_json::from_str(&content).expect("parse golden fixture")
}
