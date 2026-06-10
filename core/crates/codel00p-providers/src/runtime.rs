use crate::{ApiMode, OutputTokenParameter, ProviderCapabilities};

/// Source used for a resolved route value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteValueSource {
    RequestOverride,
    CloudProxy,
    ProviderDefault,
}

/// Provider policy decision attached to a resolved route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPolicyDecision {
    Allowed,
}

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
    pub base_url_source: RouteValueSource,
    pub credential_source: Option<String>,
    pub policy_decision: ProviderPolicyDecision,
    pub capabilities: ProviderCapabilities,
    pub models_url: Option<String>,
    pub output_token_parameter: OutputTokenParameter,
}
