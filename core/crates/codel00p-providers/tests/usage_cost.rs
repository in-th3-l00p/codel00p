use codel00p_providers::{
    ChatMessage, Credential, InferenceClient, InferenceRequest, Usage, UsagePricing,
    default_registry,
};
use httpmock::Method::POST;
use httpmock::prelude::*;
use serde_json::json;

#[test]
fn usage_estimates_cost_from_explicit_pricing() {
    let pricing = UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000)
        .with_cache_read_nanos_per_million_tokens(50_000_000)
        .with_cache_write_nanos_per_million_tokens(75_000_000)
        .with_reasoning_nanos_per_million_tokens(600_000_000);
    let usage = Usage {
        input_tokens: 7,
        output_tokens: 4,
        cache_read_tokens: 3,
        cache_write_tokens: 2,
        reasoning_tokens: 1,
    };

    let cost = usage.estimate_cost(&pricing);

    assert_eq!(cost.currency, "USD");
    assert_eq!(cost.input_nanos, 1050);
    assert_eq!(cost.output_nanos, 2400);
    assert_eq!(cost.cache_read_nanos, 150);
    assert_eq!(cost.cache_write_nanos, 150);
    assert_eq!(cost.reasoning_nanos, 600);
    assert_eq!(cost.total_nanos, 4350);
}

#[tokio::test]
async fn complete_attaches_usage_cost_when_pricing_is_supplied() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key")
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
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 3,
                    "total_tokens": 13,
                    "prompt_tokens_details": {
                        "cached_tokens": 4
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
                .message(ChatMessage::user("Say hello."))
                .pricing(
                    UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000)
                        .with_cache_read_nanos_per_million_tokens(50_000_000),
                )
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    let cost = response.cost.expect("cost should be estimated");
    assert_eq!(response.content.as_deref(), Some("hello"));
    assert_eq!(cost.currency, "USD");
    assert_eq!(cost.input_nanos, 900);
    assert_eq!(cost.output_nanos, 1800);
    assert_eq!(cost.cache_read_nanos, 200);
    assert_eq!(cost.cache_write_nanos, 0);
    assert_eq!(cost.reasoning_nanos, 0);
    assert_eq!(cost.total_nanos, 2900);
}

#[tokio::test]
async fn complete_attaches_usage_cost_from_client_model_pricing() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key");

            then.status(200).json_body(json!({
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
                    "total_tokens": 13
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .model_pricing(
            "custom",
            "test-model",
            UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    let cost = response.cost.expect("cost should be estimated");
    assert_eq!(cost.currency, "USD");
    assert_eq!(cost.input_nanos, 1500);
    assert_eq!(cost.output_nanos, 1800);
    assert_eq!(cost.total_nanos, 3300);
}

#[tokio::test]
async fn request_pricing_overrides_client_model_pricing() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key");

            then.status(200).json_body(json!({
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
                    "total_tokens": 13
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .model_pricing(
            "custom",
            "test-model",
            UsagePricing::usd_nanos_per_million_tokens(1, 1),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .pricing(UsagePricing::usd_nanos_per_million_tokens(
                    200_000_000,
                    700_000_000,
                ))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    let cost = response.cost.expect("cost should be estimated");
    assert_eq!(cost.input_nanos, 2000);
    assert_eq!(cost.output_nanos, 2100);
    assert_eq!(cost.total_nanos, 4100);
}

#[tokio::test]
async fn client_model_pricing_canonicalizes_provider_aliases() {
    let server = MockServer::start_async().await;
    let chat = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer test-key");

            then.status(200).json_body(json!({
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
                    "total_tokens": 13
                }
            }));
        })
        .await;

    let client = InferenceClient::builder()
        .registry(default_registry())
        .credential("custom", Credential::api_key("test-key"))
        .model_pricing(
            "ollama",
            "test-model",
            UsagePricing::usd_nanos_per_million_tokens(150_000_000, 600_000_000),
        )
        .build();

    let response = client
        .complete(
            InferenceRequest::builder("custom", "test-model")
                .base_url(server.base_url())
                .message(ChatMessage::user("Say hello."))
                .build(),
        )
        .await
        .unwrap();

    chat.assert_async().await;
    assert_eq!(
        response.cost.expect("cost should be estimated").total_nanos,
        3300
    );
}
