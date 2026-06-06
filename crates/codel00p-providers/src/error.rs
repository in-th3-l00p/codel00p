use thiserror::Error;

/// Errors returned by provider resolution, request building, and transports.
#[derive(Debug, Error, PartialEq, Eq)]
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
}

