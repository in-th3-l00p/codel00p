use std::collections::BTreeMap;

use crate::{
    ApiMode, Credential, InferenceRequest, InferenceResponse, ProviderError, ProviderPolicy,
    ProviderRegistry, ResolvedInferenceRoute, default_registry,
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
        let route = self.resolve(&request)?;
        let credential = self.credentials.get(&route.provider).ok_or_else(|| {
            ProviderError::MissingCredential {
                provider: route.provider.clone(),
            }
        })?;

        match route.api_mode {
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
                provider: route.provider,
                message: format!("api mode {other:?} is not implemented yet"),
            }),
        }
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

        let base_url = request
            .base_url
            .clone()
            .or_else(|| profile.default_base_url.map(str::to_string))
            .ok_or_else(|| ProviderError::MissingBaseUrl {
                provider: profile.id.to_string(),
            })?;

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
            credential_source,
            output_token_parameter: profile.output_token_parameter,
        })
    }
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
