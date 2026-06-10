use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::model_catalog::ModelCatalogWireResponse;
use crate::{
    ApiMode, AuthType, ClassifiedProviderError, Credential, InferenceRequest, InferenceResponse,
    ModelCatalogRequest, ProviderError, ProviderModel, ProviderPolicy, ProviderPolicyDecision,
    ProviderPricingCatalog, ProviderProfile, ProviderRegistry, ResolvedInferenceRoute,
    RouteValueSource, UsagePricing, classify_provider_error, default_registry,
    transports::{
        anthropic_messages::AnthropicMessagesTransport,
        azure_chat_completions::AzureChatCompletionsTransport,
        bedrock_converse::BedrockConverseTransport, chat_completions::ChatCompletionsTransport,
        gemini::GeminiTransport, responses::ResponsesTransport,
    },
};

/// High-level inference facade used by codel00p modules.
#[derive(Debug, Clone)]
pub struct InferenceClient {
    registry: ProviderRegistry,
    credentials: BTreeMap<String, StoredCredential>,
    policy: ProviderPolicy,
    model_pricing: BTreeMap<String, BTreeMap<String, UsagePricing>>,
    provider_proxies: BTreeMap<String, ProviderProxyRoute>,
}

impl InferenceClient {
    pub fn builder() -> InferenceClientBuilder {
        InferenceClientBuilder {
            registry: default_registry(),
            credentials: BTreeMap::new(),
            policy: ProviderPolicy::allow_all(),
            model_pricing: BTreeMap::new(),
            provider_proxies: BTreeMap::new(),
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
                let response =
                    self.attach_cost_estimate(response, &request, &route.provider, &request.model);
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
                    let response = self.attach_cost_estimate(
                        response,
                        &request,
                        &route.provider,
                        &fallback.model,
                    );
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

    pub async fn list_models(
        &self,
        request: ModelCatalogRequest,
    ) -> Result<Vec<ProviderModel>, ProviderError> {
        let profile = self.registry.resolve(&request.provider).ok_or_else(|| {
            ProviderError::UnknownProvider {
                provider: request.provider.clone(),
            }
        })?;
        self.policy.check_provider(profile.id)?;

        let models_url = request
            .models_url
            .clone()
            .or_else(|| {
                request
                    .base_url
                    .as_ref()
                    .map(|base_url| format!("{}/models", base_url.trim_end_matches('/')))
            })
            .or_else(|| profile.models_url.map(str::to_string))
            .ok_or_else(|| ProviderError::MissingBaseUrl {
                provider: profile.id.to_string(),
            })?;

        let credential =
            self.credentials
                .get(profile.id)
                .ok_or_else(|| ProviderError::MissingCredential {
                    provider: profile.id.to_string(),
                })?;
        let mut request_builder = reqwest::Client::new().get(models_url);
        match &credential.credential {
            Credential::ApiKey(api_key) => {
                request_builder = request_builder.bearer_auth(api_key);
            }
            Credential::None => {}
            Credential::AwsSigV4 { .. } => {
                return Err(ProviderError::InvalidResponse {
                    provider: profile.id.to_string(),
                    message: "model catalog listing does not support AWS SigV4 credentials yet"
                        .to_string(),
                });
            }
        }

        let response = request_builder
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: profile.id.to_string(),
                message: error.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Http {
                provider: profile.id.to_string(),
                message: format!("status {status}: {body}"),
            });
        }

        let wire_response = response
            .json::<ModelCatalogWireResponse>()
            .await
            .map_err(|error| ProviderError::InvalidResponse {
                provider: profile.id.to_string(),
                message: error.to_string(),
            })?;
        Ok(self
            .policy
            .filter_models(profile.id, wire_response.into_models()))
    }

    async fn complete_one(
        &self,
        request: InferenceRequest,
    ) -> Result<(ResolvedInferenceRoute, InferenceResponse), FailedRouteAttempt> {
        let route = self.resolve(&request)?;
        let Some(credential) = self.credential_for_route(&route, &request.provider) else {
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
            ApiMode::AzureChatCompletions => {
                AzureChatCompletionsTransport::new()
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

    fn credential_for_route(
        &self,
        route: &ResolvedInferenceRoute,
        requested_provider: &str,
    ) -> Option<&Credential> {
        if route.base_url_source == RouteValueSource::CloudProxy {
            return self
                .provider_proxies
                .get(&route.provider)
                .map(|proxy| &proxy.credential);
        }

        self.credentials
            .get(&route.provider)
            .or_else(|| self.credentials.get(requested_provider))
            .map(|credential| &credential.credential)
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
        } else if let Some(proxy) = self.provider_proxies.get(profile.id) {
            (proxy.base_url.clone(), RouteValueSource::CloudProxy)
        } else if let Some(base_url) = profile.default_base_url {
            (base_url.to_string(), RouteValueSource::ProviderDefault)
        } else {
            return Err(ProviderError::MissingBaseUrl {
                provider: profile.id.to_string(),
            });
        };

        let credential_source = if base_url_source == RouteValueSource::CloudProxy {
            self.provider_proxies
                .contains_key(profile.id)
                .then(|| "cloud_proxy".to_string())
        } else {
            self.credentials
                .get(profile.id)
                .or_else(|| self.credentials.get(&request.provider))
                .map(|credential| credential.source.clone())
        };

        self.policy.check_provider(profile.id)?;
        self.policy.check_model(profile.id, &request.model)?;

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

    fn attach_cost_estimate(
        &self,
        mut response: InferenceResponse,
        request: &InferenceRequest,
        provider: &str,
        model: &str,
    ) -> InferenceResponse {
        let pricing = request
            .pricing
            .as_ref()
            .or_else(|| self.model_pricing.get(provider)?.get(model));
        if let (Some(usage), Some(pricing)) = (&response.usage, pricing) {
            response.cost = Some(usage.estimate_cost(pricing));
        }
        response
    }
}

#[derive(Debug)]
struct FailedRouteAttempt {
    route: Option<ResolvedInferenceRoute>,
    error: ProviderError,
}

#[derive(Debug, Clone)]
struct StoredCredential {
    credential: Credential,
    source: String,
}

impl StoredCredential {
    fn configured(credential: Credential) -> Self {
        Self {
            credential,
            source: "configured".to_string(),
        }
    }

    fn environment(source_key: impl Into<String>, credential: Credential) -> Self {
        Self {
            credential,
            source: format!("environment:{}", source_key.into()),
        }
    }
}

#[derive(Debug, Clone)]
struct ProviderProxyRoute {
    base_url: String,
    credential: Credential,
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
    credentials: BTreeMap<String, StoredCredential>,
    policy: ProviderPolicy,
    model_pricing: BTreeMap<String, BTreeMap<String, UsagePricing>>,
    provider_proxies: BTreeMap<String, ProviderProxyRoute>,
}

impl InferenceClientBuilder {
    pub fn registry(mut self, registry: ProviderRegistry) -> Self {
        self.registry = registry;
        self
    }

    pub fn credential(mut self, provider: impl Into<String>, credential: Credential) -> Self {
        self.credentials
            .insert(provider.into(), StoredCredential::configured(credential));
        self
    }

    pub fn credentials_from_env(mut self) -> Self {
        let loaded_credentials: Vec<_> = self
            .registry
            .profiles()
            .filter_map(|profile| {
                env_credential_for_profile(profile)
                    .map(|credential| (profile.id.to_string(), credential))
            })
            .collect();

        for (provider, credential) in loaded_credentials {
            self.credentials.entry(provider).or_insert(credential);
        }

        self
    }

    pub fn policy(mut self, policy: ProviderPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn model_pricing(
        mut self,
        provider: impl Into<String>,
        model: impl Into<String>,
        pricing: UsagePricing,
    ) -> Self {
        self.model_pricing
            .entry(provider.into())
            .or_default()
            .insert(model.into(), pricing);
        self
    }

    pub fn pricing_catalog(mut self, catalog: ProviderPricingCatalog) -> Self {
        for entry in catalog.entries {
            self.model_pricing
                .entry(entry.provider)
                .or_default()
                .insert(entry.model, entry.pricing);
        }
        self
    }

    pub fn provider_proxy(
        mut self,
        provider: impl Into<String>,
        base_url: impl Into<String>,
        credential: Credential,
    ) -> Self {
        self.provider_proxies.insert(
            provider.into(),
            ProviderProxyRoute {
                base_url: base_url.into(),
                credential,
            },
        );
        self
    }

    pub fn build(self) -> InferenceClient {
        let policy = self.policy.canonicalize(&self.registry);
        let model_pricing = self
            .model_pricing
            .into_iter()
            .map(|(provider, prices)| {
                let canonical = self
                    .registry
                    .resolve(&provider)
                    .map(|profile| profile.id.to_string())
                    .unwrap_or(provider);
                (canonical, prices)
            })
            .collect();
        let provider_proxies = self
            .provider_proxies
            .into_iter()
            .map(|(provider, proxy)| {
                let canonical = self
                    .registry
                    .resolve(&provider)
                    .map(|profile| profile.id.to_string())
                    .unwrap_or(provider);
                (canonical, proxy)
            })
            .collect();
        let credentials = self
            .credentials
            .into_iter()
            .map(|(provider, stored)| {
                let canonical = self
                    .registry
                    .resolve(&provider)
                    .map(|profile| profile.id.to_string())
                    .unwrap_or(provider);
                (canonical, stored)
            })
            .collect();
        InferenceClient {
            registry: self.registry,
            credentials,
            policy,
            model_pricing,
            provider_proxies,
        }
    }
}

fn env_credential_for_profile(profile: &ProviderProfile) -> Option<StoredCredential> {
    match profile.auth_type {
        AuthType::ApiKey | AuthType::GitHubCopilot | AuthType::Custom => {
            let (source, secret) = read_first_env_secret(profile.env_vars)?;
            Some(StoredCredential::environment(
                source,
                Credential::api_key(secret),
            ))
        }
        AuthType::AwsSdk => aws_sigv4_env_credential(),
        AuthType::OAuthExternal | AuthType::CloudProxy => None,
    }
}

fn aws_sigv4_env_credential() -> Option<StoredCredential> {
    let (source, access_key_id) =
        read_first_env_secret(&["CODEL00P_PROVIDER_AWS_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID"])?;
    let (_, secret_access_key) = read_first_env_secret(&[
        "CODEL00P_PROVIDER_AWS_SECRET_ACCESS_KEY",
        "AWS_SECRET_ACCESS_KEY",
    ])?;
    let session_token =
        read_first_env_secret(&["CODEL00P_PROVIDER_AWS_SESSION_TOKEN", "AWS_SESSION_TOKEN"])
            .map(|(_, value)| value);
    let (_, region) = read_first_env_secret(&[
        "CODEL00P_PROVIDER_AWS_REGION",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ])?;

    Some(StoredCredential::environment(
        source,
        Credential::aws_sigv4(
            access_key_id,
            secret_access_key,
            session_token.as_deref(),
            region,
        ),
    ))
}

fn read_first_env_secret(keys: &[&str]) -> Option<(String, String)> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| ((*key).to_string(), value))
    })
}
