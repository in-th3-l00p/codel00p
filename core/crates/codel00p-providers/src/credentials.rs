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
}
