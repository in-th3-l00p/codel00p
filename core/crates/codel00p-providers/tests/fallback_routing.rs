use codel00p_protocol::RuntimeErrorKind;
use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn falls_back_when_primary_route_is_rate_limited() {
    let primary_server = MockServer::start_async().await;
    let fallback_server = MockServer::start_async().await;

    let primary = primary_server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer openrouter-key")
                .body_includes(r#""model":"anthropic/claude-sonnet""#);
            then.status(429).json_body(json!({
                "error": {"message": "rate limit exceeded"}
            }));
        })
        .await;
    let fallback = fallback_server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer custom-key")
                .body_includes(r#""model":"local-model""#)
                .body_includes("hello");
            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "fallback ok"}
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("openrouter-key"))
        .credential("custom", Credential::api_key("custom-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .base_url(primary_server.base_url())
                .message(ChatMessage::user("hello"))
                .fallback_route_with_base_url("custom", "local-model", fallback_server.base_url())
                .build(),
        )
        .await
        .unwrap();

    primary.assert_async().await;
    fallback.assert_async().await;
    assert_eq!(response.content.as_deref(), Some("fallback ok"));

    let route = response
        .provider_data
        .get("codel00p_route")
        .expect("route metadata should be attached");
    assert_eq!(route["selected"]["provider"], json!("custom"));
    assert_eq!(route["selected"]["model"], json!("local-model"));
    assert_eq!(route["attempts"][0]["provider"], json!("openrouter"));
    assert_eq!(route["attempts"][0]["outcome"], json!("fallback"));
    assert_eq!(
        route["attempts"][0]["error_kind"],
        json!(format!("{:?}", RuntimeErrorKind::ProviderRateLimit))
    );
    assert_eq!(route["attempts"][1]["provider"], json!("custom"));
    assert_eq!(route["attempts"][1]["outcome"], json!("success"));
}

#[tokio::test]
async fn does_not_fallback_for_non_fallbackable_errors() {
    let primary_server = MockServer::start_async().await;
    let fallback_server = MockServer::start_async().await;

    let primary = primary_server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(401).json_body(json!({
                "error": {"message": "invalid api key"}
            }));
        })
        .await;
    let fallback = fallback_server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "should not run"}
                }]
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("openrouter", Credential::api_key("openrouter-key"))
        .credential("custom", Credential::api_key("custom-key"))
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("openrouter", "anthropic/claude-sonnet")
                .base_url(primary_server.base_url())
                .message(ChatMessage::user("hello"))
                .fallback_route_with_base_url("custom", "local-model", fallback_server.base_url())
                .build(),
        )
        .await
        .unwrap_err();

    primary.assert_async().await;
    fallback.assert_calls_async(0).await;
    assert!(error.to_string().contains("401"));
}
