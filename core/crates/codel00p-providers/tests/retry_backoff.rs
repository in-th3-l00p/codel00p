//! End-to-end retry-with-backoff behavior for the inference client: a transient
//! rate limit is retried on the same route (honoring `Retry-After`) before
//! falling back, and non-retryable errors are never retried.

use std::time::Duration;

use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, ProviderPolicy, RetryPolicy,
    default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn retries_the_same_route_then_falls_back() {
    let primary_server = MockServer::start_async().await;
    let fallback_server = MockServer::start_async().await;

    // The primary always rate-limits, advertising an immediate Retry-After so the
    // test does not actually sleep.
    let primary = primary_server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(429)
                .header("retry-after", "0")
                .json_body(json!({ "error": { "message": "rate limit exceeded" } }));
        })
        .await;
    let fallback = fallback_server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
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
        .policy(ProviderPolicy::allow_all())
        // Two retries with negligible backoff keeps the test fast and
        // deterministic.
        .retry_policy(RetryPolicy::new(
            2,
            Duration::from_millis(1),
            Duration::from_secs(1),
        ))
        .credential("openrouter", Credential::api_key("primary-key"))
        .credential("custom", Credential::api_key("fallback-key"))
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("openrouter", "primary-model")
                .base_url(primary_server.base_url())
                .message(ChatMessage::user("hello"))
                .fallback_route_with_base_url(
                    "custom",
                    "fallback-model",
                    fallback_server.base_url(),
                )
                .build(),
        )
        .await
        .unwrap();

    // Primary tried 1 + 2 retries = 3 times before the single fallback succeeds.
    primary.assert_calls_async(3).await;
    fallback.assert_calls_async(1).await;
    assert_eq!(response.content.as_deref(), Some("fallback ok"));

    let route = response
        .provider_data
        .get("codel00p_route")
        .expect("route metadata");
    assert_eq!(route["attempts"][0]["provider"], json!("openrouter"));
    assert_eq!(route["attempts"][0]["outcome"], json!("fallback"));
    // The honored Retry-After hint is surfaced in the failed attempt's metadata.
    assert_eq!(route["attempts"][0]["retry_after_secs"], json!(0));
    assert_eq!(route["attempts"][1]["provider"], json!("custom"));
    assert_eq!(route["attempts"][1]["outcome"], json!("success"));
}

#[tokio::test]
async fn does_not_retry_non_retryable_errors() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(401)
                .json_body(json!({ "error": { "message": "invalid api key" } }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .policy(ProviderPolicy::allow_all())
        .retry_policy(RetryPolicy::new(
            5,
            Duration::from_millis(1),
            Duration::from_secs(1),
        ))
        .credential("openrouter", Credential::api_key("primary-key"))
        .build();

    let error = client
        .complete(
            InferenceRequest::builder("openrouter", "primary-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("hello"))
                .build(),
        )
        .await
        .unwrap_err();

    // A 401 is not retryable, so the route is tried exactly once despite a
    // generous retry budget.
    endpoint.assert_calls_async(1).await;
    assert!(error.to_string().contains("401"));
}
