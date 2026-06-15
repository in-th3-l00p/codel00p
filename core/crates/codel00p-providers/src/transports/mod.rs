pub(crate) mod anthropic_messages;
pub(crate) mod azure_chat_completions;
pub(crate) mod bedrock_converse;
pub(crate) mod chat_completions;
pub(crate) mod gemini;
pub(crate) mod responses;
pub(crate) mod sse;

use crate::ProviderError;

/// Reads a `Retry-After` header expressed as an integer number of seconds — the
/// common form for HTTP 429/503 responses. Returns `None` when the header is
/// absent or uses the HTTP-date form (which providers rarely send for rate
/// limits), so callers treat an unparseable hint as simply "no hint".
pub(crate) fn retry_after_seconds(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

/// Builds a `ProviderError::Http` for a non-success response, folding in a
/// `Retry-After` hint when present so the error classifier and route-audit
/// metadata can surface the provider's suggested backoff. When there is no hint
/// the message is unchanged from the plain `status <code>: <body>` form.
pub(crate) fn http_status_error(
    provider: &str,
    status: reqwest::StatusCode,
    body: &str,
    retry_after: Option<u64>,
) -> ProviderError {
    let message = match retry_after {
        Some(secs) => format!("status {status} (retry-after: {secs}s): {body}"),
        None => format!("status {status}: {body}"),
    };
    ProviderError::Http {
        provider: provider.to_string(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn parses_integer_retry_after_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("30"));
        assert_eq!(retry_after_seconds(&headers), Some(30));
    }

    #[test]
    fn ignores_absent_or_non_integer_retry_after() {
        assert_eq!(retry_after_seconds(&HeaderMap::new()), None);
        let mut headers = HeaderMap::new();
        // HTTP-date form is not parsed into seconds.
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(retry_after_seconds(&headers), None);
    }

    #[test]
    fn error_message_includes_hint_only_when_present() {
        let with = http_status_error(
            "openai",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "slow down",
            Some(12),
        );
        let ProviderError::Http { message, .. } = with else {
            panic!("expected Http error");
        };
        assert!(message.contains("retry-after: 12s"));
        assert!(message.contains("429"));

        let without = http_status_error(
            "openai",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "slow down",
            None,
        );
        let ProviderError::Http { message, .. } = without else {
            panic!("expected Http error");
        };
        assert!(!message.contains("retry-after"));
    }
}
