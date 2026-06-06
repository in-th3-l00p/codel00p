use std::collections::BTreeMap;

use crate::{
    Credential, InferenceRequest, InferenceResponse, ProviderError, ProviderRegistry,
    default_registry,
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

        let _base_url = request
            .base_url
            .as_deref()
            .or(profile.default_base_url)
            .ok_or_else(|| ProviderError::MissingBaseUrl {
                provider: profile.id.to_string(),
            })?;

        let _credential = self.credentials.get(profile.id);

        Err(ProviderError::MissingCredential {
            provider: profile.id.to_string(),
        })
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

