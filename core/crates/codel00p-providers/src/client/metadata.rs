//! Route-attempt metadata and fallback error bookkeeping.

use super::*;

#[derive(Debug)]
pub(super) struct FailedRouteAttempt {
    pub(super) route: Option<ResolvedInferenceRoute>,
    pub(super) error: ProviderError,
}

impl From<ProviderError> for FailedRouteAttempt {
    fn from(error: ProviderError) -> Self {
        Self { route: None, error }
    }
}

pub(super) fn attach_route_metadata(
    mut response: InferenceResponse,
    route: &ResolvedInferenceRoute,
    model: &str,
    attempts: Vec<Value>,
) -> InferenceResponse {
    response.provider_data.insert(
        "codel00p_route".to_string(),
        json!({
            "selected": route_metadata(route, model),
            "attempts": attempts,
        }),
    );
    response
}

fn route_metadata(route: &ResolvedInferenceRoute, model: &str) -> Value {
    json!({
        "requested_provider": route.requested_provider,
        "provider": route.provider,
        "model": model,
        "api_mode": format!("{:?}", route.api_mode),
        "auth_type": format!("{:?}", route.auth_type),
        "base_url_source": format!("{:?}", route.base_url_source),
        "credential_source": route.credential_source,
        "credential_source_kind": route.credential_source_kind,
        "credential_kind": route.credential_kind,
        "policy_decision": format!("{:?}", route.policy_decision),
        "policy": &route.policy,
        "models_url": route.models_url,
        "output_token_parameter": format!("{:?}", route.output_token_parameter),
        "capabilities": {
            "tools": route.capabilities.tools,
            "streaming": route.capabilities.streaming,
            "vision": route.capabilities.vision,
            "reasoning": route.capabilities.reasoning,
        },
    })
}

pub(super) fn successful_attempt(route: &ResolvedInferenceRoute, model: &str) -> Value {
    let mut attempt = route_metadata(route, model);
    attempt["outcome"] = Value::String("success".to_string());
    attempt
}

pub(super) fn failed_attempt(
    route: Option<&ResolvedInferenceRoute>,
    fallback_provider: &str,
    model: &str,
    classified: &ClassifiedProviderError,
    outcome: &str,
) -> Value {
    let mut attempt = if let Some(route) = route {
        route_metadata(route, model)
    } else {
        json!({
            "requested_provider": fallback_provider,
            "provider": fallback_provider,
            "model": model,
        })
    };
    attempt["outcome"] = Value::String(outcome.to_string());
    attempt["error_kind"] = Value::String(format!("{:?}", classified.kind()));
    if let Some(secs) = classified.retry_after_secs() {
        attempt["retry_after_secs"] = json!(secs);
    }
    attempt
}
