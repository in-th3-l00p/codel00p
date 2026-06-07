use codel00p_protocol::RuntimeErrorKind;

use crate::ProviderError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedProviderError {
    kind: RuntimeErrorKind,
    retryable: bool,
    should_compress: bool,
    should_rotate_credential: bool,
    should_fallback: bool,
}

impl ClassifiedProviderError {
    pub fn new(kind: RuntimeErrorKind) -> Self {
        Self {
            kind,
            retryable: false,
            should_compress: false,
            should_rotate_credential: false,
            should_fallback: false,
        }
    }

    pub fn with_retryable(mut self) -> Self {
        self.retryable = true;
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
        return ClassifiedProviderError::new(RuntimeErrorKind::ProviderRateLimit)
            .with_retryable()
            .with_fallback();
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
        return ClassifiedProviderError::new(RuntimeErrorKind::ProviderUnavailable)
            .with_retryable()
            .with_fallback();
    }

    ClassifiedProviderError::new(RuntimeErrorKind::Unknown)
}

fn contains_any(message: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| message.contains(pattern))
}
