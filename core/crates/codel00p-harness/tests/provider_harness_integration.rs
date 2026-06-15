use std::fs;

use codel00p_harness::{
    AgentHarness, ProviderModelClient, SessionId, SessionMessage, ToolRegistry, UserMessage,
    Workspace,
};
use codel00p_providers::{Credential, InferenceClient, default_registry};
use httpmock::{Method::POST, prelude::*};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn harness_runs_final_turn_through_provider_inference_client() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "user", "content": "Summarize this project."}
                    ],
                    "tools": [
                        {
                            "type": "function",
                            "function": {
                                "name": "list_files",
                                "description": "List files inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "properties": { "path": { "type": "string" } }
                                }
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "description": "Read a UTF-8 file inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "required": ["path"],
                                    "properties": { "path": { "type": "string" } }
                                }
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "search_text",
                                "description": "Search UTF-8 files inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "required": ["query"],
                                    "properties": {
                                        "query": { "type": "string" },
                                        "path": { "type": "string" }
                                    }
                                }
                            }
                        }
                    ]
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "This is codel00p."
                    }
                }]
            }));
        })
        .await;

    let dir = tempdir().expect("tempdir");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();
    let model_client =
        ProviderModelClient::new(client, "custom", "test-model").with_base_url(server.base_url());

    let outcome = AgentHarness::builder()
        .model_client(model_client)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("provider-final"),
            UserMessage::new("Summarize this project."),
        )
        .await
        .expect("run turn");

    chat.assert_async().await;
    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("This is codel00p.")
    );
    assert_eq!(
        outcome.session_state.messages(),
        &[
            SessionMessage::user("Summarize this project."),
            SessionMessage::assistant("This is codel00p."),
        ]
    );
}

#[tokio::test]
async fn harness_executes_provider_tool_call_and_sends_tool_result_back_to_inference() {
    let server = MockServer::start_async().await;
    let first_call = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "user", "content": "Read README."}
                    ],
                    "tools": [
                        {
                            "type": "function",
                            "function": {
                                "name": "list_files",
                                "description": "List files inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "properties": { "path": { "type": "string" } }
                                }
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "description": "Read a UTF-8 file inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "required": ["path"],
                                    "properties": { "path": { "type": "string" } }
                                }
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "search_text",
                                "description": "Search UTF-8 files inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "required": ["query"],
                                    "properties": {
                                        "query": { "type": "string" },
                                        "path": { "type": "string" }
                                    }
                                }
                            }
                        }
                    ]
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-readme",
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "arguments": "{\"path\":\"README.md\"}"
                            }
                        }]
                    }
                }]
            }));
        })
        .await;
    let second_call = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "user", "content": "Read README."},
                        {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call-readme",
                                "type": "function",
                                "function": {
                                    "name": "read_file",
                                    "arguments": "{\"path\":\"README.md\"}"
                                }
                            }]
                        },
                        {
                            "role": "tool",
                            "content": "{\"content\":\"Agent Harness\\n\",\"path\":\"README.md\"}",
                            "tool_call_id": "call-readme"
                        }
                    ],
                    "tools": [
                        {
                            "type": "function",
                            "function": {
                                "name": "list_files",
                                "description": "List files inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "properties": { "path": { "type": "string" } }
                                }
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "description": "Read a UTF-8 file inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "required": ["path"],
                                    "properties": { "path": { "type": "string" } }
                                }
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "search_text",
                                "description": "Search UTF-8 files inside the workspace root.",
                                "parameters": {
                                    "type": "object",
                                    "required": ["query"],
                                    "properties": {
                                        "query": { "type": "string" },
                                        "path": { "type": "string" }
                                    }
                                }
                            }
                        }
                    ]
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "README says Agent Harness."
                    }
                }]
            }));
        })
        .await;

    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("README.md"), "Agent Harness\n").expect("write readme");
    let workspace = Workspace::new(dir.path()).expect("workspace");
    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();
    let model_client =
        ProviderModelClient::new(client, "custom", "test-model").with_base_url(server.base_url());

    let outcome = AgentHarness::builder()
        .model_client(model_client)
        .workspace(workspace)
        .tools(ToolRegistry::read_only_defaults())
        .max_iterations(4)
        .build()
        .expect("build harness")
        .run_turn(
            SessionId::from_static("provider-tool"),
            UserMessage::new("Read README."),
        )
        .await
        .expect("run turn");

    first_call.assert_async().await;
    second_call.assert_async().await;
    assert_eq!(
        outcome.assistant_message.as_deref(),
        Some("README says Agent Harness.")
    );
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "read_file");
}
