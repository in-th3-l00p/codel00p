use thiserror::Error;

/// Errors returned by provider resolution, request building, and transports.
#[derive(Debug, Error)]
pub enum ProviderError {
    /// The requested provider is unknown to the active registry.
    #[error("unknown provider `{provider}`")]
    UnknownProvider { provider: String },

    /// A provider needs a base URL but neither the profile nor request supplied one.
    #[error("provider `{provider}` requires a base URL")]
    MissingBaseUrl { provider: String },

    /// A provider request needs credentials and none were available.
    #[error("provider `{provider}` requires credentials")]
    MissingCredential { provider: String },

    /// The active policy rejected the provider route.
    #[error("provider `{provider}` was denied by policy: {reason}")]
    PolicyDenied { provider: String, reason: String },

    /// The provider returned a response that could not be parsed into codel00p's shape.
    #[error("invalid response from provider `{provider}`: {message}")]
    InvalidResponse { provider: String, message: String },

    /// HTTP request failed before a provider response could be normalized.
    #[error("http request to provider `{provider}` failed: {message}")]
    Http { provider: String, message: String },
}
