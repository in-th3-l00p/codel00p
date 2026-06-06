use crate::ApiMode;

/// Resolved runtime route for an inference request.
///
/// This is intentionally safe to show in logs and UI. It records whether a
/// credential source exists, but never exposes the secret value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInferenceRoute {
    pub requested_provider: String,
    pub provider: String,
    pub api_mode: ApiMode,
    pub base_url: String,
    pub credential_source: Option<String>,
}
