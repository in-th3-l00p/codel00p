use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderError, ResponseFormat,
    TokenSink, ToolCall, ToolChoice, ToolDefinition, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;
use std::sync::Mutex;

/// A recorded tool-call delta: (index, id, name, args_fragment).
type RecordedDelta = (usize, Option<String>, Option<String>, String);

/// A test sink that records every tool-call argument delta it receives, so a
/// test can assert the fragments arrive incrementally and in order. Text tokens
/// are ignored.
#[derive(Default)]
struct RecordingSink {
    deltas: Mutex<Vec<RecordedDelta>>,
}

impl TokenSink for RecordingSink {
    fn on_token(&self, _token: &str) {}

    fn on_tool_call_delta(
        &self,
        index: usize,
        id: Option<&str>,
        name: Option<&str>,
        args_fragment: &str,
    ) {
        self.deltas.lock().unwrap().push((
            index,
            id.map(str::to_string),
            name.map(str::to_string),
            args_fragment.to_string(),
        ));
    }
}

#[tokio::test]
async fn chat_completions_serializes_json_schema_response_format() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .body_includes(r#""response_format""#)
                .body_includes(r#""type":"json_schema""#)
                .body_includes(r#""strict":true"#);
            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": { "role": "assistant", "content": "{}" }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("give me json"))
                .response_format(ResponseFormat::JsonSchema {
                    name: "result".into(),
                    schema: json!({ "type": "object" }),
                })
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
}

#[tokio::test]
async fn chat_completions_posts_openai_compatible_payload_and_normalizes_response() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "system", "content": "You are precise."},
                        {"role": "user", "content": "Say hello."}
                    ],
                    "temperature": 0.2,
                    "max_tokens": 128
                }));

            then.status(200).json_body(json!({
                "id": "chatcmpl_test",
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello"
                    }
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 3,
                    "total_tokens": 13,
                    "prompt_tokens_details": {
                        "cached_tokens": 4
                    },
                    "completion_tokens_details": {
                        "reasoning_tokens": 2
                    }
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::system("You are precise."))
                .message(ChatMessage::user("Say hello."))
                .temperature(0.2)
                .max_output_tokens(128)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 6);
    assert_eq!(usage.output_tokens, 3);
    assert_eq!(usage.cache_read_tokens, 4);
    assert_eq!(usage.reasoning_tokens, 2);
}

#[tokio::test]
async fn chat_completions_serializes_tool_choice() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .body_includes(r#""tool_choice":"required""#)
                .body_includes(r#""name":"echo""#);
            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": { "role": "assistant", "content": "ok" }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("hi"))
                .tool(ToolDefinition::function(
                    "echo",
                    "Echo the input.",
                    json!({ "type": "object" }),
                ))
                .tool_choice(ToolChoice::Required)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
}

#[tokio::test]
async fn chat_completions_requires_credentials_for_remote_requests() {
    let server = MockServer::start_async().await;
    let client = InferenceClient::builder()
        .registry(default_registry())
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::MissingCredential { provider } if provider == "custom"));
}

#[tokio::test]
async fn cloud_proxy_routes_chat_completions_through_configured_gateway() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/proxy/custom/chat/completions")
                .header("authorization", "Bearer proxy-key")
                .json_body(json!({
                    "model": "test-model",
                    "messages": [
                        {"role": "user", "content": "Say hello."}
                    ]
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello"
                    }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .provider_proxy(
            "custom",
            format!("{}/proxy/custom", server.base_url()),
            Credential::api_key("proxy-key"),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
}

#[tokio::test]
async fn github_copilot_uses_max_completion_tokens() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .json_body(json!({
                    "model": "gpt-5.4-mini",
                    "messages": [
                        {"role": "user", "content": "Say hello."}
                    ],
                    "max_completion_tokens": 64
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello"
                    }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("github", "gpt-5.4-mini")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
}

#[tokio::test]
async fn github_models_uses_models_endpoint_and_max_tokens() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/inference/chat/completions")
                .header("authorization", "Bearer github-models-key")
                .json_body(json!({
                    "model": "openai/gpt-4.1-mini",
                    "messages": [
                        {"role": "user", "content": "Say hello."}
                    ],
                    "max_tokens": 64
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hello"
                    }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("github-models", Credential::api_key("github-models-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("github-models", "openai/gpt-4.1-mini")
                .base_url(format!("{}/inference", server.base_url()))
                .message(ChatMessage::user("Say hello."))
                .max_output_tokens(64)
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("hello"));
}

#[tokio::test]
async fn chat_completions_sends_tool_call_ids_for_tool_result_messages() {
    let server = MockServer::start_async().await;
    let chat = server
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
                            "content": "{\"content\":\"Agent Harness\"}",
                            "tool_call_id": "call-readme"
                        }
                    ]
                }));

            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "done"
                    }
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Read README."))
                .message(ChatMessage::assistant_tool_calls(vec![ToolCall {
                    id: Some("call-readme".to_string()),
                    name: "read_file".to_string(),
                    arguments: json!({ "path": "README.md" }),
                    provider_data: Default::default(),
                }]))
                .message(ChatMessage::tool_result(
                    "call-readme",
                    r#"{"content":"Agent Harness"}"#,
                ))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("done"));
}

#[tokio::test]
async fn chat_completions_streams_token_deltas_and_assembles_response() {
    use std::sync::{Arc, Mutex};

    let server = MockServer::start_async().await;
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2}}\n\n",
        "data: [DONE]\n\n",
    );
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key")
                .body_includes(r#""stream":true"#);
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse);
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let tokens: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = tokens.clone();
    let sink = move |token: &str| collector.lock().unwrap().push(token.to_string());

    let response = client
        .complete_streaming(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
            &sink,
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(*tokens.lock().unwrap(), vec!["Hel", "lo"]);
    assert_eq!(response.content.as_deref(), Some("Hello"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(response.usage.unwrap().output_tokens, 2);
}

#[tokio::test]
async fn chat_completions_streams_and_assembles_tool_call_deltas() {
    let server = MockServer::start_async().await;
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\"\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":\\\"a.txt\\\"}\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let chat = server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse);
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let sink = |_: &str| {};
    let response = client
        .complete_streaming(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Read a.txt."))
                .build(),
            &sink,
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(response.tool_calls.len(), 1);
    let call: &ToolCall = &response.tool_calls[0];
    assert_eq!(call.id.as_deref(), Some("call_1"));
    assert_eq!(call.name, "read_file");
    assert_eq!(call.arguments, json!({ "path": "a.txt" }));
    assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
}

#[tokio::test]
async fn chat_completions_surfaces_tool_call_argument_deltas_to_sink() {
    let server = MockServer::start_async().await;
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\"\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":\\\"a.txt\\\"}\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let chat = server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse);
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .build();

    let sink = RecordingSink::default();
    let response = client
        .complete_streaming(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Read a.txt."))
                .build(),
            &sink,
        )
        .await
        .unwrap();

    chat.assert_async().await;

    // The sink saw the call assembled incrementally: the opening fragment with
    // id+name, then two argument-only fragments, in order.
    let deltas = sink.deltas.lock().unwrap();
    assert_eq!(
        *deltas,
        vec![
            (
                0,
                Some("call_1".to_string()),
                Some("read_file".to_string()),
                String::new()
            ),
            (0, Some("call_1".to_string()), None, "{\"path\"".to_string()),
            (
                0,
                Some("call_1".to_string()),
                None,
                ":\"a.txt\"}".to_string()
            ),
        ]
    );
    // Concatenating the fragments reconstructs the raw argument JSON.
    let reconstructed: String = deltas.iter().map(|(_, _, _, frag)| frag.as_str()).collect();
    assert_eq!(reconstructed, "{\"path\":\"a.txt\"}");

    // The final assembled tool call is unchanged.
    assert_eq!(response.tool_calls.len(), 1);
    let call: &ToolCall = &response.tool_calls[0];
    assert_eq!(call.id.as_deref(), Some("call_1"));
    assert_eq!(call.name, "read_file");
    assert_eq!(call.arguments, json!({ "path": "a.txt" }));
    assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
}
