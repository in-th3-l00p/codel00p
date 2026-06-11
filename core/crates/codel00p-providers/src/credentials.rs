use crate::ProviderError;

/// Resolved provider credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    ApiKey(String),
    AwsSigV4 {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        region: String,
    },
    None,
}

/// Safe credential kind metadata for audit surfaces.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum CredentialKind {
    ApiKey,
    AwsSigV4,
    None,
}

/// Safe credential source category metadata for audit surfaces.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum CredentialSourceKind {
    Configured,
    Environment,
    Organization,
    CloudProxy,
    ManagedIdentity,
}

/// Provider credential loaded with safe source metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProviderCredential {
    pub credential: Credential,
    pub source: String,
    pub source_kind: CredentialSourceKind,
}

/// Request passed to managed identity credential resolvers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedIdentityCredentialRequest<'a> {
    provider: &'a str,
    identity_ref: &'a str,
}

impl<'a> ManagedIdentityCredentialRequest<'a> {
    pub fn new(provider: &'a str, identity_ref: &'a str) -> Self {
        Self {
            provider,
            identity_ref,
        }
    }

    pub fn provider(&self) -> &'a str {
        self.provider
    }

    pub fn identity_ref(&self) -> &'a str {
        self.identity_ref
    }
}

/// Resolves a provider credential from a managed identity reference.
pub trait ManagedIdentityCredentialResolver {
    fn resolve(
        &self,
        request: ManagedIdentityCredentialRequest<'_>,
    ) -> Result<Credential, ProviderError>;
}

impl Credential {
    pub fn api_key(value: impl Into<String>) -> Self {
        Self::ApiKey(value.into())
    }

    pub fn aws_sigv4(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        session_token: Option<&str>,
        region: impl Into<String>,
    ) -> Self {
        Self::AwsSigV4 {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: session_token.map(str::to_string),
            region: region.into(),
        }
    }

    pub fn kind(&self) -> CredentialKind {
        match self {
            Self::ApiKey(_) => CredentialKind::ApiKey,
            Self::AwsSigV4 { .. } => CredentialKind::AwsSigV4,
            Self::None => CredentialKind::None,
        }
    }
}
