use std::collections::BTreeMap;

use crate::{
    ApiMode, Credential, InferenceRequest, InferenceResponse, ProviderError, ProviderRegistry,
    default_registry, transports::chat_completions::ChatCompletionsTransport,
};

/// High-level inference facade used by codel00p modules.
#[derive(Debug, Clone)]
pub struct InferenceClient {
    registry: ProviderRegistry,
    credentials: BTreeMap<String, Credential>,
}

impl InferenceClient {
    pub fn builder() -> InferenceClientBuilder {
        InferenceClientBuilder {
            registry: default_registry(),
            credentials: BTreeMap::new(),
        }
    }

    pub async fn complete(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, ProviderError> {
        let profile =
            self.registry
                .resolve(&request.provider)
                .ok_or_else(|| ProviderError::UnknownProvider {
                    provider: request.provider.clone(),
                })?;

        let base_url = request
            .base_url
            .clone()
            .or_else(|| profile.default_base_url.map(str::to_string))
            .ok_or_else(|| ProviderError::MissingBaseUrl {
                provider: profile.id.to_string(),
            })?;

        let credential = self
            .credentials
            .get(profile.id)
            .or_else(|| self.credentials.get(&request.provider))
            .ok_or_else(|| ProviderError::MissingCredential {
                provider: profile.id.to_string(),
            })?;

        match profile.api_mode {
            ApiMode::ChatCompletions => {
                ChatCompletionsTransport::new()
                    .complete(profile.id, &base_url, credential, request)
                    .await
            }
            other => Err(ProviderError::InvalidResponse {
                provider: profile.id.to_string(),
                message: format!("api mode {other:?} is not implemented yet"),
            }),
        }
    }
}

/// Builder for [`InferenceClient`].
#[derive(Debug, Clone)]
pub struct InferenceClientBuilder {
    registry: ProviderRegistry,
    credentials: BTreeMap<String, Credential>,
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

    pub fn build(self) -> InferenceClient {
        InferenceClient {
            registry: self.registry,
            credentials: self.credentials,
        }
    }
}
