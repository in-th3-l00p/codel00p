use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{
    ApiMode, ClassifiedProviderError, Credential, InferenceRequest, InferenceResponse,
    ProviderError, ProviderPolicy, ProviderPolicyDecision, ProviderRegistry,
    ResolvedInferenceRoute, RouteValueSource, classify_provider_error, default_registry,
    transports::{
        anthropic_messages::AnthropicMessagesTransport, bedrock_converse::BedrockConverseTransport,
        chat_completions::ChatCompletionsTransport, gemini::GeminiTransport,
        responses::ResponsesTransport,
    },
};

/// High-level inference facade used by codel00p modules.
#[derive(Debug, Clone)]
pub struct InferenceClient {
    registry: ProviderRegistry,
    credentials: BTreeMap<String, Credential>,
    policy: ProviderPolicy,
}

impl InferenceClient {
    pub fn builder() -> InferenceClientBuilder {
        InferenceClientBuilder {
            registry: default_registry(),
            credentials: BTreeMap::new(),
            policy: ProviderPolicy::allow_all(),
        }
    }

    pub async fn complete(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, ProviderError> {
        let fallback_routes = request.fallback_routes.clone();
        let mut attempts = Vec::new();
        let mut last_error = match self.complete_one(request.clone()).await {
            Ok((route, response)) => {
                attempts.push(successful_attempt(&route, &request.model));
                let response = attach_cost_estimate(response, &request);
                return Ok(attach_route_metadata(
                    response,
                    &route,
                    &request.model,
                    attempts,
                ));
            }
            Err(failed) => {
                let classified = classify_provider_error(&failed.error);
                attempts.push(failed_attempt(
                    failed.route.as_ref(),
                    &request.provider,
                    &request.model,
                    &classified,
                    if classified.should_fallback() && !fallback_routes.is_empty() {
                        "fallback"
                    } else {
                        "error"
                    },
                ));
                if !classified.should_fallback() {
                    return Err(failed.error);
                }
                failed.error
            }
        };

        for fallback in fallback_routes {
            let mut fallback_request = request.clone();
            fallback_request.provider = fallback.provider.clone();
            fallback_request.model = fallback.model.clone();
            fallback_request.base_url = fallback.base_url.clone();
            fallback_request.fallback_routes.clear();

            match self.complete_one(fallback_request).await {
                Ok((route, response)) => {
                    attempts.push(successful_attempt(&route, &fallback.model));
                    let response = attach_cost_estimate(response, &request);
                    return Ok(attach_route_metadata(
                        response,
                        &route,
                        &fallback.model,
                        attempts,
                    ));
                }
                Err(failed) => {
                    let classified = classify_provider_error(&failed.error);
                    attempts.push(failed_attempt(
                        failed.route.as_ref(),
                        &fallback.provider,
                        &fallback.model,
                        &classified,
                        if classified.should_fallback() {
                            "fallback"
                        } else {
                            "error"
                        },
                    ));
                    if !classified.should_fallback() {
                        return Err(failed.error);
                    }
                    last_error = failed.error;
                }
            }
        }

        Err(last_error)
    }

    async fn complete_one(
        &self,
        request: InferenceRequest,
    ) -> Result<(ResolvedInferenceRoute, InferenceResponse), FailedRouteAttempt> {
        let route = self.resolve(&request)?;
        let Some(credential) = self.credentials.get(&route.provider) else {
            return Err(FailedRouteAttempt {
                route: Some(route.clone()),
                error: ProviderError::MissingCredential {
                    provider: route.provider.clone(),
                },
            });
        };

        let response = match route.api_mode {
            ApiMode::ChatCompletions => {
                ChatCompletionsTransport::new()
                    .complete(
                        &route.provider,
                        &route.base_url,
                        credential,
                        route.output_token_parameter,
                        request,
                    )
                    .await
            }
            ApiMode::AnthropicMessages => {
                AnthropicMessagesTransport::new()
                    .complete(&route.provider, &route.base_url, credential, request)
                    .await
            }
            ApiMode::Responses => {
                ResponsesTransport::new()
                    .complete(&route.provider, &route.base_url, credential, request)
                    .await
            }
            ApiMode::BedrockConverse => {
                BedrockConverseTransport::new()
                    .complete(&route.provider, &route.base_url, credential, request)
                    .await
            }
            ApiMode::Gemini => {
                GeminiTransport::new()
                    .complete(&route.provider, &route.base_url, credential, request)
                    .await
            }
            other => Err(ProviderError::InvalidResponse {
                provider: route.provider.clone(),
                message: format!("api mode {other:?} is not implemented yet"),
            }),
        }
        .map_err(|error| FailedRouteAttempt {
            route: Some(route.clone()),
            error,
        })?;

        Ok((route, response))
    }

    /// Resolve provider, API mode, base URL, and credential presence without sending a request.
    pub fn resolve(
        &self,
        request: &InferenceRequest,
    ) -> Result<ResolvedInferenceRoute, ProviderError> {
        let profile = self.registry.resolve(&request.provider).ok_or_else(|| {
            ProviderError::UnknownProvider {
                provider: request.provider.clone(),
            }
        })?;

        let (base_url, base_url_source) = if let Some(base_url) = request.base_url.clone() {
            (base_url, RouteValueSource::RequestOverride)
        } else if let Some(base_url) = profile.default_base_url {
            (base_url.to_string(), RouteValueSource::ProviderDefault)
        } else {
            return Err(ProviderError::MissingBaseUrl {
                provider: profile.id.to_string(),
            });
        };

        let credential_source = self
            .credentials
            .get(profile.id)
            .or_else(|| self.credentials.get(&request.provider))
            .map(|_| "configured".to_string());

        self.policy.check_provider(profile.id)?;

        Ok(ResolvedInferenceRoute {
            requested_provider: request.provider.clone(),
            provider: profile.id.to_string(),
            api_mode: profile.api_mode,
            base_url,
            base_url_source,
            credential_source,
            policy_decision: ProviderPolicyDecision::Allowed,
            capabilities: profile.capabilities,
            models_url: profile.models_url.map(str::to_string),
            output_token_parameter: profile.output_token_parameter,
        })
    }
}

fn attach_cost_estimate(
    mut response: InferenceResponse,
    request: &InferenceRequest,
) -> InferenceResponse {
    if let (Some(usage), Some(pricing)) = (&response.usage, &request.pricing) {
        response.cost = Some(usage.estimate_cost(pricing));
    }
    response
}

#[derive(Debug)]
struct FailedRouteAttempt {
    route: Option<ResolvedInferenceRoute>,
    error: ProviderError,
}

impl From<ProviderError> for FailedRouteAttempt {
    fn from(error: ProviderError) -> Self {
        Self { route: None, error }
    }
}

fn attach_route_metadata(
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
        "base_url_source": format!("{:?}", route.base_url_source),
        "credential_source": route.credential_source,
        "policy_decision": format!("{:?}", route.policy_decision),
    })
}

fn successful_attempt(route: &ResolvedInferenceRoute, model: &str) -> Value {
    let mut attempt = route_metadata(route, model);
    attempt["outcome"] = Value::String("success".to_string());
    attempt
}

fn failed_attempt(
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
    attempt
}

/// Builder for [`InferenceClient`].
#[derive(Debug, Clone)]
pub struct InferenceClientBuilder {
    registry: ProviderRegistry,
    credentials: BTreeMap<String, Credential>,
    policy: ProviderPolicy,
}

impl InferenceClientBuilder {
    pub fn registry(mut self, registry: ProviderRegistry) -> Self {
        self.registry = registry;
        self
    }

    pub fn credential(mut self, provider: impl Into<String>, credential: Credential) -> Self {
        self.credentials.insert(provider.into(), credential);
        self
    }

    pub fn policy(mut self, policy: ProviderPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn build(self) -> InferenceClient {
        let policy = self.policy.canonicalize(&self.registry);
        let credentials = self
            .credentials
            .into_iter()
            .map(|(provider, credential)| {
                let canonical = self
                    .registry
                    .resolve(&provider)
                    .map(|profile| profile.id.to_string())
                    .unwrap_or(provider);
                (canonical, credential)
            })
            .collect();
        InferenceClient {
            registry: self.registry,
            credentials,
            policy,
        }
    }
}
