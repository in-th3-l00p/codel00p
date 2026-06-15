use codel00p_harness::{
    HarnessInferenceRequest, MemoryPromptAssembler, ModelToolCall, ProjectInstruction,
    ProjectInstructions, ProjectMemoryContext, ProjectMemoryItem, ProviderModelClient, SessionId,
    SessionState, ToolSpec, UserMessage,
};
use codel00p_protocol::MemoryKind;
use codel00p_providers::{InferenceResponse, MessageRole, ToolCall};
use serde_json::json;

#[test]
fn maps_harness_session_messages_to_provider_request() {
    let mut state = SessionState::new(SessionId::from_static("session-provider"));
    state.push_user(UserMessage::new("Read the README."));
    state.push_assistant("I need to inspect it.");
    state.push_tool_result("call-1", "read_file", r#"{"content":"Agent Harness"}"#);
    let read_schema = json!({
        "type": "object",
        "properties": { "path": { "type": "string" } },
        "required": ["path"]
    });
    let request = HarnessInferenceRequest::new(state).with_runtime_context(
        "/workspace",
        vec![
            ToolSpec::new(
                "read_file",
                "Read a file from the workspace.",
                read_schema.clone(),
            ),
            ToolSpec::new(
                "search_text",
                "Search files for a query.",
                json!({ "type": "object" }),
            ),
        ],
    );

    let provider_request =
        ProviderModelClient::build_provider_request("github", "gpt-4o", &request);

    assert_eq!(provider_request.provider, "github");
    assert_eq!(provider_request.model, "gpt-4o");
    assert_eq!(provider_request.messages.len(), 3);
    assert_eq!(provider_request.messages[0].role, MessageRole::User);
    assert_eq!(provider_request.messages[1].role, MessageRole::Assistant);
    assert_eq!(provider_request.messages[2].role, MessageRole::Tool);
    assert_eq!(
        provider_request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["read_file", "search_text"]
    );
    // The real schema and description reach the provider — not the old stub.
    assert_eq!(
        provider_request.tools[0].description,
        "Read a file from the workspace."
    );
    assert_eq!(provider_request.tools[0].parameters, read_schema);
    assert_eq!(
        provider_request.tools[1].description,
        "Search files for a query."
    );
}

#[test]
fn assembles_project_memory_into_stable_prompt_block() {
    let memory = ProjectMemoryContext::new(vec![
        ProjectMemoryItem::new(
            "mem-b",
            MemoryKind::Workflow,
            "Run pnpm verify before pushing main.",
            vec!["verify".to_string()],
            "matched kind workflow",
        ),
        ProjectMemoryItem::new(
            "mem-a",
            MemoryKind::Architecture,
            "The harness owns tool execution.",
            vec!["harness".to_string(), "runtime".to_string()],
            "matched tag harness",
        ),
    ]);

    let prompt = MemoryPromptAssembler
        .assemble(&memory)
        .expect("memory prompt");

    assert_eq!(
        prompt,
        "\
Project memory:
- id: mem-a
  kind: architecture
  tags: harness,runtime
  reason: matched tag harness
  content: The harness owns tool execution.
- id: mem-b
  kind: workflow
  tags: verify
  reason: matched kind workflow
  content: Run pnpm verify before pushing main."
    );
}

#[test]
fn assembles_multiline_project_memory_without_breaking_prompt_fields() {
    let memory = ProjectMemoryContext::new(vec![ProjectMemoryItem::new(
        "mem-multi",
        MemoryKind::Troubleshooting,
        "First line\nSecond line",
        vec!["debug".to_string()],
        "matched content\nmatched tag debug",
    )]);

    let prompt = MemoryPromptAssembler
        .assemble(&memory)
        .expect("memory prompt");

    assert_eq!(
        prompt,
        "\
Project memory:
- id: mem-multi
  kind: troubleshooting
  tags: debug
  reason: matched content
    matched tag debug
  content: First line
    Second line"
    );
}

#[test]
fn provider_request_includes_project_memory_before_session_messages() {
    let mut state = SessionState::new(SessionId::from_static("session-provider"));
    state.push_user(UserMessage::new("Inspect the project."));
    let request =
        HarnessInferenceRequest::new(state).with_project_memory(ProjectMemoryContext::new(vec![
            ProjectMemoryItem::new(
                "mem-harness",
                MemoryKind::Architecture,
                "The harness owns tool execution.",
                vec!["harness".to_string()],
                "matched tag harness",
            ),
        ]));

    let provider_request =
        ProviderModelClient::build_provider_request("github", "gpt-4o", &request);

    assert_eq!(provider_request.messages.len(), 2);
    assert_eq!(provider_request.messages[0].role, MessageRole::System);
    assert_eq!(
        provider_request.messages[0].content.as_deref(),
        Some(
            "\
Project memory:
- id: mem-harness
  kind: architecture
  tags: harness
  reason: matched tag harness
  content: The harness owns tool execution."
        )
    );
    assert_eq!(provider_request.messages[1].role, MessageRole::User);
}

#[test]
fn provider_request_includes_project_instructions_before_memory_and_session_messages() {
    let mut state = SessionState::new(SessionId::from_static("session-provider"));
    state.push_user(UserMessage::new("Inspect the project."));
    let request = HarnessInferenceRequest::new(state)
        .with_project_instructions(ProjectInstructions::new(vec![ProjectInstruction::new(
            "CODEL00P.md",
            "Always run pnpm verify.",
        )]))
        .with_project_memory(ProjectMemoryContext::new(vec![ProjectMemoryItem::new(
            "mem-harness",
            MemoryKind::Architecture,
            "The harness owns tool execution.",
            vec!["harness".to_string()],
            "matched tag harness",
        )]));

    let provider_request =
        ProviderModelClient::build_provider_request("github", "gpt-4o", &request);

    assert_eq!(provider_request.messages.len(), 3);
    assert_eq!(provider_request.messages[0].role, MessageRole::System);
    assert_eq!(
        provider_request.messages[0].content.as_deref(),
        Some(
            "\
Project instructions:
## CODEL00P.md
Always run pnpm verify."
        )
    );
    assert_eq!(provider_request.messages[1].role, MessageRole::System);
    assert!(
        provider_request.messages[1]
            .content
            .as_deref()
            .expect("memory prompt")
            .starts_with("Project memory:")
    );
    assert_eq!(provider_request.messages[2].role, MessageRole::User);
}

#[test]
fn empty_project_memory_adds_no_provider_message() {
    let mut state = SessionState::new(SessionId::from_static("session-provider"));
    state.push_user(UserMessage::new("Inspect the project."));
    let request =
        HarnessInferenceRequest::new(state).with_project_memory(ProjectMemoryContext::new(vec![]));

    let provider_request =
        ProviderModelClient::build_provider_request("github", "gpt-4o", &request);

    assert_eq!(provider_request.messages.len(), 1);
    assert_eq!(provider_request.messages[0].role, MessageRole::User);
}

#[test]
fn maps_provider_response_to_harness_response() {
    let provider_response = InferenceResponse {
        content: Some("Need the README.".to_string()),
        tool_calls: vec![ToolCall {
            id: Some("call-provider".to_string()),
            name: "read_file".to_string(),
            arguments: json!({ "path": "README.md" }),
            provider_data: Default::default(),
        }],
        finish_reason: Some("tool_calls".to_string()),
        reasoning: None,
        usage: None,
        cost: None,
        provider_data: Default::default(),
    };

    let response =
        ProviderModelClient::map_provider_response("github", "gpt-4o", provider_response);

    assert_eq!(response.provider(), "github");
    assert_eq!(response.model(), "gpt-4o");
    assert_eq!(response.assistant_message(), Some("Need the README."));
    assert_eq!(
        response.tool_calls(),
        &[ModelToolCall::new(
            "call-provider",
            "read_file",
            json!({ "path": "README.md" }),
        )]
    );
    assert_eq!(response.finish_reason(), Some("tool_calls"));
}
