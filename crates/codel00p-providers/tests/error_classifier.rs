use codel00p_protocol::RuntimeErrorKind;
use codel00p_providers::{ProviderError, classify_provider_error};

#[test]
fn classifies_missing_credentials_as_provider_auth() {
    let classified = classify_provider_error(&ProviderError::MissingCredential {
        provider: "openai".to_string(),
    });

    assert_eq!(classified.kind(), RuntimeErrorKind::ProviderAuth);
    assert!(classified.should_rotate_credential());
    assert!(!classified.should_compress());
}

#[test]
fn classifies_rate_limit_http_errors_as_retryable() {
    let classified = classify_provider_error(&ProviderError::Http {
        provider: "openrouter".to_string(),
        message: "HTTP 429: rate limit exceeded".to_string(),
    });

    assert_eq!(classified.kind(), RuntimeErrorKind::ProviderRateLimit);
    assert!(classified.retryable());
    assert!(classified.should_fallback());
}

#[test]
fn classifies_context_overflow_as_compression_signal() {
    let classified = classify_provider_error(&ProviderError::Http {
        provider: "anthropic".to_string(),
        message: "maximum context length exceeded".to_string(),
    });

    assert_eq!(classified.kind(), RuntimeErrorKind::ContextOverflow);
    assert!(classified.should_compress());
    assert!(!classified.should_rotate_credential());
}

#[test]
fn classifies_payload_and_model_errors() {
    let payload = classify_provider_error(&ProviderError::Http {
        provider: "custom".to_string(),
        message: "413 payload too large".to_string(),
    });
    let model = classify_provider_error(&ProviderError::InvalidResponse {
        provider: "custom".to_string(),
        message: "model not found".to_string(),
    });

    assert_eq!(payload.kind(), RuntimeErrorKind::PayloadTooLarge);
    assert!(payload.should_compress());
    assert_eq!(model.kind(), RuntimeErrorKind::ModelUnavailable);
    assert!(model.should_fallback());
}

#[test]
fn unknown_provider_errors_are_not_assumed_recoverable() {
    let classified = classify_provider_error(&ProviderError::UnknownProvider {
        provider: "ghost".to_string(),
    });

    assert_eq!(classified.kind(), RuntimeErrorKind::Unknown);
    assert!(!classified.retryable());
    assert!(!classified.should_fallback());
}
