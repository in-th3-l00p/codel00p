use codel00p_protocol::RuntimeErrorKind;

use crate::ProviderError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedProviderError {
    kind: RuntimeErrorKind,
    retryable: bool,
    should_compress: bool,
    should_rotate_credential: bool,
    should_fallback: bool,
    retry_after_secs: Option<u64>,
}

impl ClassifiedProviderError {
    pub fn new(kind: RuntimeErrorKind) -> Self {
        Self {
            kind,
            retryable: false,
            should_compress: false,
            should_rotate_credential: false,
            should_fallback: false,
            retry_after_secs: None,
        }
    }

    pub fn with_retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    /// Records the provider's `Retry-After` hint (seconds) so retry/backoff can
    /// honour it and route-audit metadata can surface it.
    pub fn with_retry_after(mut self, secs: u64) -> Self {
        self.retry_after_secs = Some(secs);
        self
    }

    pub fn with_compression(mut self) -> Self {
        self.should_compress = true;
        self
    }

    pub fn with_credential_rotation(mut self) -> Self {
        self.should_rotate_credential = true;
        self
    }

    pub fn with_fallback(mut self) -> Self {
        self.should_fallback = true;
        self
    }

    pub fn kind(&self) -> RuntimeErrorKind {
        self.kind
    }

    pub fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn should_compress(&self) -> bool {
        self.should_compress
    }

    pub fn should_rotate_credential(&self) -> bool {
        self.should_rotate_credential
    }

    pub fn should_fallback(&self) -> bool {
        self.should_fallback
    }

    /// The provider's suggested backoff in seconds, when it sent a `Retry-After`.
    pub fn retry_after_secs(&self) -> Option<u64> {
        self.retry_after_secs
    }
}

pub fn classify_provider_error(error: &ProviderError) -> ClassifiedProviderError {
    match error {
        ProviderError::MissingCredential { .. } => {
            ClassifiedProviderError::new(RuntimeErrorKind::ProviderAuth).with_credential_rotation()
        }
        ProviderError::PolicyDenied { .. } => {
            ClassifiedProviderError::new(RuntimeErrorKind::PermissionDenied)
        }
        ProviderError::MissingBaseUrl { .. } | ProviderError::UnknownProvider { .. } => {
            ClassifiedProviderError::new(RuntimeErrorKind::Unknown)
        }
        ProviderError::InvalidResponse { message, .. } | ProviderError::Http { message, .. } => {
            classify_message(message)
        }
    }
}

fn classify_message(message: &str) -> ClassifiedProviderError {
    let message = message.to_ascii_lowercase();
    // A provider-supplied backoff hint applies to whichever retryable kind the
    // message classifies as (rate limit or transient unavailability).
    let retry_after = parse_retry_after(&message);
    let with_hint = |classified: ClassifiedProviderError| match retry_after {
        Some(secs) => classified.with_retry_after(secs),
        None => classified,
    };

    if contains_any(
        &message,
        &["context length", "maximum context", "too many tokens"],
    ) {
        return ClassifiedProviderError::new(RuntimeErrorKind::ContextOverflow).with_compression();
    }

    if contains_any(
        &message,
        &["payload too large", "request entity too large", "413"],
    ) {
        return ClassifiedProviderError::new(RuntimeErrorKind::PayloadTooLarge).with_compression();
    }

    if contains_any(
        &message,
        &["rate limit", "too many requests", "429", "throttled"],
    ) {
        return with_hint(
            ClassifiedProviderError::new(RuntimeErrorKind::ProviderRateLimit)
                .with_retryable()
                .with_fallback(),
        );
    }

    if contains_any(&message, &["401", "403", "unauthorized", "invalid api key"]) {
        return ClassifiedProviderError::new(RuntimeErrorKind::ProviderAuth)
            .with_credential_rotation();
    }

    if contains_any(
        &message,
        &["model not found", "unknown model", "invalid model"],
    ) {
        return ClassifiedProviderError::new(RuntimeErrorKind::ModelUnavailable).with_fallback();
    }

    if contains_any(
        &message,
        &["503", "529", "overloaded", "temporarily unavailable"],
    ) {
        return with_hint(
            ClassifiedProviderError::new(RuntimeErrorKind::ProviderUnavailable)
                .with_retryable()
                .with_fallback(),
        );
    }

    ClassifiedProviderError::new(RuntimeErrorKind::Unknown)
}

fn contains_any(message: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| message.contains(pattern))
}

/// Extracts an integer `retry-after: <n>s` hint that the transports fold into
/// the error message from the HTTP header. Returns the first such value, or
/// `None` when absent. The message is already lowercased by the caller.
fn parse_retry_after(message: &str) -> Option<u64> {
    let after = message.split("retry-after:").nth(1)?;
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(message: &str) -> ProviderError {
        ProviderError::Http {
            provider: "openai".to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn parse_retry_after_reads_integer_seconds() {
        assert_eq!(
            parse_retry_after("status 429 (retry-after: 30s): busy"),
            Some(30)
        );
        assert_eq!(parse_retry_after("retry-after: 5s"), Some(5));
        assert_eq!(parse_retry_after("status 429: slow down"), None);
        assert_eq!(parse_retry_after("retry-after: soon"), None);
    }

    #[test]
    fn rate_limit_carries_retry_after_hint() {
        let classified =
            classify_provider_error(&http("status 429 (retry-after: 12s): too many requests"));
        assert_eq!(classified.kind(), RuntimeErrorKind::ProviderRateLimit);
        assert!(classified.retryable());
        assert!(classified.should_fallback());
        assert_eq!(classified.retry_after_secs(), Some(12));
    }

    #[test]
    fn unavailable_carries_retry_after_hint() {
        let classified = classify_provider_error(&http("status 503 (retry-after: 2s): overloaded"));
        assert_eq!(classified.kind(), RuntimeErrorKind::ProviderUnavailable);
        assert_eq!(classified.retry_after_secs(), Some(2));
    }

    #[test]
    fn rate_limit_without_hint_has_no_retry_after() {
        let classified = classify_provider_error(&http("status 429: too many requests"));
        assert_eq!(classified.kind(), RuntimeErrorKind::ProviderRateLimit);
        assert_eq!(classified.retry_after_secs(), None);
    }
}
