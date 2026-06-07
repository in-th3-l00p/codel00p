use codel00p_protocol::{
    AgentEvent, CompactionRecord, ContextWindowState, EventId, MemoryEntry, MemoryKind,
    MemorySource, MemoryStatus, PermissionDecision, PermissionMode, PermissionRequest,
    PermissionScope, ProjectRef, ProtocolVersion, ProviderRef, RuntimeErrorKind, SessionId,
    SessionMessage, SessionPersistenceEvent, SessionRole, ToolCall, ToolProgress, ToolResult,
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

#[test]
fn runtime_error_kinds_drive_recovery_boundaries() {
    let kinds = [
        RuntimeErrorKind::ProviderAuth,
        RuntimeErrorKind::ProviderRateLimit,
        RuntimeErrorKind::ContextOverflow,
        RuntimeErrorKind::PermissionDenied,
        RuntimeErrorKind::ToolExecution,
        RuntimeErrorKind::Cancelled,
    ];

    let encoded = serde_json::to_value(kinds).expect("serialize error kinds");

    assert_eq!(
        encoded,
        json!([
            "provider_auth",
            "provider_rate_limit",
            "context_overflow",
            "permission_denied",
            "tool_execution",
            "cancelled"
        ])
    );
}

#[test]
fn permission_contracts_capture_decision_flow() {
    let request = PermissionRequest::new(
        "permission-1",
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
        "write_file",
        json!({ "path": "src/lib.rs" }),
        PermissionScope::WorkspaceWrite,
    )
    .with_reason("tool wants to modify the workspace");
    let decision = PermissionDecision::deny(
        request.id(),
        PermissionMode::Ask,
        "write access has not been approved",
    );

    assert_eq!(request.tool_name(), "write_file");
    assert_eq!(request.scope(), PermissionScope::WorkspaceWrite);
    assert_eq!(decision.request_id(), "permission-1");
    assert_eq!(decision.mode(), PermissionMode::Ask);
    assert!(!decision.allows_execution());
}

#[test]
fn context_and_compaction_contracts_track_pressure() {
    let window = ContextWindowState::new("gpt-4.1", 200_000, 181_000)
        .with_warning_threshold(170_000)
        .with_blocking_threshold(197_000);
    let compaction = CompactionRecord::new(
        EventId::from_static("event-compact"),
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
        42,
        12,
    )
    .with_summary("Preserved active task, relevant files, and pending work.");

    assert_eq!(window.model(), "gpt-4.1");
    assert_eq!(window.used_tokens(), 181_000);
    assert!(window.is_above_warning_threshold());
    assert!(!window.is_at_blocking_limit());
    assert_eq!(compaction.before_message_count(), 42);
    assert_eq!(compaction.after_message_count(), 12);
}

#[test]
fn tool_progress_and_session_persistence_are_protocol_events() {
    let progress = ToolProgress::new(
        EventId::from_static("event-progress"),
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
        "search_text",
        "started",
    )
    .with_message("searching workspace");
    let persisted = SessionPersistenceEvent::record_appended(
        EventId::from_static("event-record"),
        SessionId::from_static("session-1"),
        "record-1",
        3,
    );

    assert_eq!(progress.tool_name(), "search_text");
    assert_eq!(progress.phase(), "started");
    assert_eq!(persisted.record_id(), "record-1");
    assert_eq!(persisted.sequence(), 3);
}
