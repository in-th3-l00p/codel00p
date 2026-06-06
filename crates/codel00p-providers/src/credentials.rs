/// Resolved provider credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    ApiKey(String),
    None,
}

impl Credential {
    pub fn api_key(value: impl Into<String>) -> Self {
        Self::ApiKey(value.into())
    }
}

