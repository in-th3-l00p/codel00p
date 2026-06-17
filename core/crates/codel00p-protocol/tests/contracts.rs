use codel00p_protocol::{
    Agent, AgentEvent, AgentUpdate, CompactionRecord, ContextWindowState, EventId, McpServer,
    McpServerUpdate, McpTransport, MemoryAuditEntry, MemoryEntry, MemoryKind, MemoryReviewAction,
    MemorySensitivity, MemorySource, MemoryStatus, MemoryVisibility, NewAgent, NewMcpServer,
    NewMemoryCandidate, NewProject, OrgMember, OrgRef, OrgRole, PermissionDecision, PermissionMode,
    PermissionRequest, PermissionScope, Project, ProjectRef, ProjectUpdate, ProtocolVersion,
    ProviderRef, RuntimeErrorKind, SessionId, SessionMessage, SessionPersistenceEvent, SessionRole,
    ToolCall, ToolProgress, ToolResult, TurnId, Viewer,
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
    assert_eq!(memory.sensitivity(), MemorySensitivity::Normal);
    assert_eq!(memory.tags(), &["harness"]);
}

#[test]
fn memory_sources_can_carry_explicit_source_uri() {
    let source = MemorySource::turn(
        SessionId::from_static("session-1"),
        TurnId::from_static("turn-1"),
    )
    .with_uri("https://github.com/in-th3-l00p/codel00p/pull/1");

    let encoded = serde_json::to_value(&source).expect("serialize source");

    assert_eq!(source.session_id().as_str(), "session-1");
    assert_eq!(source.turn_id().as_str(), "turn-1");
    assert_eq!(
        source.uri(),
        Some("https://github.com/in-th3-l00p/codel00p/pull/1")
    );
    assert_eq!(
        encoded,
        json!({
            "session_id": "session-1",
            "turn_id": "turn-1",
            "uri": "https://github.com/in-th3-l00p/codel00p/pull/1"
        })
    );
}

#[test]
fn memory_entries_default_to_project_visibility_and_omit_it_when_serialized() {
    let memory = MemoryEntry::new(
        "mem-default",
        ProjectRef::new("project-1", "codel00p"),
        MemoryKind::Workflow,
        "Default scoped knowledge.",
    );
    assert_eq!(memory.visibility(), MemoryVisibility::Project);

    // Default visibility is skipped on serialize, keeping the wire format
    // backward compatible with pre-visibility records.
    let encoded = serde_json::to_value(&memory).expect("serialize memory");
    assert!(encoded.get("visibility").is_none());

    let decoded: MemoryEntry = serde_json::from_value(encoded).expect("decode memory");
    assert_eq!(decoded.visibility(), MemoryVisibility::Project);
}

#[test]
fn memory_entries_can_widen_visibility_scope() {
    let memory = MemoryEntry::new(
        "mem-org",
        ProjectRef::new("project-1", "codel00p"),
        MemoryKind::Workflow,
        "Org-wide knowledge.",
    )
    .with_visibility(MemoryVisibility::Org);

    let encoded = serde_json::to_value(&memory).expect("serialize memory");
    assert_eq!(encoded["visibility"], "org");

    let decoded: MemoryEntry = serde_json::from_value(encoded).expect("decode memory");
    assert_eq!(decoded.visibility(), MemoryVisibility::Org);
}

#[test]
fn memory_entries_can_mark_sensitive_project_knowledge() {
    let memory = MemoryEntry::new(
        "mem-sensitive",
        ProjectRef::new("project-1", "codel00p"),
        MemoryKind::Workflow,
        "Use the private deployment credential only from CI.",
    )
    .with_sensitivity(MemorySensitivity::Sensitive);

    assert_eq!(memory.sensitivity(), MemorySensitivity::Sensitive);
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

    let event = AgentEvent::ContextCompacted {
        event_id: EventId::from_static("event-context-compacted"),
        session_id: SessionId::from_static("session-1"),
        turn_id: TurnId::from_static("turn-1"),
        before_message_count: 42,
        after_message_count: 12,
        summary: Some("Preserved active task.".to_string()),
    };
    let encoded = serde_json::to_value(event).expect("serialize compaction event");
    assert_eq!(encoded["kind"], "context_compacted");
    assert_eq!(encoded["before_message_count"], 42);
    assert_eq!(encoded["after_message_count"], 12);
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

#[test]
fn org_roles_normalize_from_clerk_claims() {
    assert_eq!(OrgRole::from_clerk_claim("org:admin"), Some(OrgRole::Admin));
    assert_eq!(OrgRole::from_clerk_claim("admin"), Some(OrgRole::Admin));
    assert_eq!(
        OrgRole::from_clerk_claim("org:member"),
        Some(OrgRole::Member)
    );
    assert_eq!(OrgRole::from_clerk_claim("guest"), None);
    assert!(OrgRole::Admin.is_admin());
    assert!(!OrgRole::Member.is_admin());

    let encoded = serde_json::to_value([OrgRole::Admin, OrgRole::Member]).expect("serialize roles");
    assert_eq!(encoded, json!(["admin", "member"]));
}

#[test]
fn org_member_projects_clerk_membership() {
    let member = OrgMember::new("user_123", OrgRole::Admin)
        .with_email("dev@team.dev")
        .with_name("Dev User");

    let encoded = serde_json::to_value(&member).expect("serialize member");
    let decoded: OrgMember = serde_json::from_value(encoded.clone()).expect("deserialize member");

    assert_eq!(decoded, member);
    assert_eq!(member.user_id(), "user_123");
    assert_eq!(member.role(), OrgRole::Admin);
    assert_eq!(member.email(), Some("dev@team.dev"));
    assert_eq!(member.name(), Some("Dev User"));
    assert_eq!(
        encoded,
        json!({
            "user_id": "user_123",
            "role": "admin",
            "email": "dev@team.dev",
            "name": "Dev User"
        })
    );
}

#[test]
fn viewer_carries_clerk_identity_and_org_context() {
    let viewer = Viewer::new("user_123").with_email("dev@team.dev").with_org(
        OrgRef::new("org_123", "Acme Engineering").with_slug("acme"),
        OrgRole::Admin,
    );

    let encoded = serde_json::to_value(&viewer).expect("serialize viewer");
    let decoded: Viewer = serde_json::from_value(encoded.clone()).expect("deserialize viewer");

    assert_eq!(decoded, viewer);
    assert_eq!(viewer.user_id(), "user_123");
    assert_eq!(viewer.email(), Some("dev@team.dev"));
    assert_eq!(viewer.org().map(|org| org.id()), Some("org_123"));
    assert_eq!(viewer.org().and_then(|org| org.slug()), Some("acme"));
    assert_eq!(viewer.org_role(), Some(OrgRole::Admin));
    assert_eq!(encoded["org_role"], "admin");
}

#[test]
fn viewer_without_org_omits_optional_fields() {
    let viewer = Viewer::new("user_solo");
    let encoded = serde_json::to_value(&viewer).expect("serialize viewer");

    assert_eq!(encoded, json!({ "user_id": "user_solo" }));
}

#[test]
fn projects_are_org_owned_product_data() {
    let project = Project::new("project-1", "org_123", "codel00p", "codel00p")
        .with_repository_url("https://github.com/in-th3-l00p/codel00p");

    let encoded = serde_json::to_value(&project).expect("serialize project");
    let decoded: Project = serde_json::from_value(encoded.clone()).expect("deserialize project");

    assert_eq!(decoded, project);
    assert_eq!(project.org_id(), "org_123");
    assert_eq!(project.slug(), "codel00p");
    assert_eq!(
        project.repository_url(),
        Some("https://github.com/in-th3-l00p/codel00p")
    );
    assert_eq!(encoded["org_id"], "org_123");
}

#[test]
fn new_memory_candidate_defaults_and_round_trips() {
    let request: NewMemoryCandidate = serde_json::from_value(json!({
        "kind": "convention",
        "content": "Run cargo from core/."
    }))
    .expect("deserialize candidate");

    assert_eq!(request.kind, MemoryKind::Convention);
    assert_eq!(request.tags, Vec::<String>::new());
    assert_eq!(request.sensitivity, MemorySensitivity::Normal);
    assert_eq!(request.source_uri, None);

    let full = NewMemoryCandidate {
        tags: vec!["testing".into()],
        sensitivity: MemorySensitivity::Sensitive,
        source_uri: Some("https://example.com/pr/1".into()),
        ..NewMemoryCandidate::new(MemoryKind::Workflow, "Use the deploy script.")
    };
    let encoded = serde_json::to_value(&full).expect("serialize candidate");
    assert_eq!(encoded["kind"], "workflow");
    assert_eq!(encoded["sensitivity"], "sensitive");
    assert_eq!(encoded["source_uri"], "https://example.com/pr/1");
}

#[test]
fn memory_review_actions_map_to_statuses() {
    assert_eq!(
        MemoryReviewAction::Approved.resulting_status(),
        Some(MemoryStatus::Approved)
    );
    assert_eq!(
        MemoryReviewAction::Rejected.resulting_status(),
        Some(MemoryStatus::Rejected)
    );

    let entry = MemoryAuditEntry::new("mem-1", MemoryReviewAction::Approved, "user_admin");
    let encoded = serde_json::to_value(&entry).expect("serialize audit entry");
    assert_eq!(
        encoded,
        json!({ "memory_id": "mem-1", "action": "approved", "actor": "user_admin" })
    );
    let decoded: MemoryAuditEntry = serde_json::from_value(encoded).expect("deserialize");
    assert_eq!(decoded, entry);
}

#[test]
fn new_project_request_omits_owning_org() {
    let request: NewProject =
        serde_json::from_value(json!({ "name": "codel00p" })).expect("deserialize new project");

    assert_eq!(request.name, "codel00p");
    assert_eq!(request.repository_url, None);

    let with_repo = NewProject {
        name: "codel00p".to_string(),
        repository_url: Some("https://example.com/repo".to_string()),
    };
    let encoded = serde_json::to_value(&with_repo).expect("serialize new project");
    assert_eq!(
        encoded,
        json!({ "name": "codel00p", "repository_url": "https://example.com/repo" })
    );
}

#[test]
fn project_update_omits_absent_fields() {
    let update: ProjectUpdate =
        serde_json::from_value(json!({ "name": "renamed" })).expect("deserialize");
    assert_eq!(update.name.as_deref(), Some("renamed"));
    assert_eq!(update.repository_url, None);
    assert_eq!(
        serde_json::to_value(ProjectUpdate::default()).expect("serialize"),
        json!({})
    );
}

#[test]
fn agents_reference_mcp_servers_and_round_trip() {
    let agent = Agent::new(
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
    .with_mcp_server_ids(vec!["mcp_1".to_string()]);

    let encoded = serde_json::to_value(&agent).expect("serialize agent");
    let decoded: Agent = serde_json::from_value(encoded.clone()).expect("deserialize agent");

    assert_eq!(decoded, agent);
    assert_eq!(agent.provider(), "anthropic");
    assert_eq!(agent.mcp_server_ids(), &["mcp_1"]);
    assert_eq!(encoded["created_by"], "user_admin");

    // New-agent defaults to no MCP servers; update is sparse.
    let new: NewAgent = serde_json::from_value(
        json!({ "name": "Doc agent", "provider": "openai", "model": "gpt-5" }),
    )
    .expect("deserialize new agent");
    assert_eq!(new.mcp_server_ids, Vec::<String>::new());
    assert_eq!(
        serde_json::to_value(AgentUpdate {
            model: Some("claude-opus-4-8".into()),
            ..AgentUpdate::default()
        })
        .expect("serialize update"),
        json!({ "model": "claude-opus-4-8" })
    );
}

#[test]
fn mcp_servers_capture_transport_and_round_trip() {
    let server = McpServer::new(
        "mcp_1",
        "org_123",
        "proj_1",
        "GitHub",
        McpTransport::Http,
        "user_admin",
    )
    .with_url("https://mcp.github.example/sse");

    let encoded = serde_json::to_value(&server).expect("serialize mcp");
    let decoded: McpServer = serde_json::from_value(encoded.clone()).expect("deserialize mcp");

    assert_eq!(decoded, server);
    assert_eq!(server.transport(), McpTransport::Http);
    assert_eq!(server.url(), Some("https://mcp.github.example/sse"));
    assert!(server.enabled());
    assert_eq!(encoded["transport"], "http");

    // New-mcp defaults enabled to true; transport round-trips both variants.
    let new: NewMcpServer = serde_json::from_value(json!({
        "name": "Local fs",
        "transport": "stdio",
        "command": "codel00p-mcp-fs"
    }))
    .expect("deserialize new mcp");
    assert_eq!(new.transport, McpTransport::Stdio);
    assert!(new.enabled);
    assert_eq!(
        serde_json::to_value(McpServerUpdate {
            enabled: Some(false),
            ..McpServerUpdate::default()
        })
        .expect("serialize update"),
        json!({ "enabled": false })
    );
}
