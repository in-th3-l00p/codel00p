use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CredentialKind, CredentialSourceKind, ProviderCapabilities, ProviderError, ProviderModel,
    ProviderModelCatalogPolicy, ProviderRegistry, ProviderRoutePolicy,
};

const ENTERPRISE_DIRECT_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "azure-foundry",
    "bedrock",
    "gemini",
    "github",
    "github-models",
];

/// Provider policy enforced during route resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPolicy {
    allowed_providers: Option<BTreeSet<String>>,
    allowed_models: BTreeMap<String, BTreeSet<String>>,
    allowed_credential_kinds: BTreeMap<String, BTreeSet<CredentialKind>>,
    allowed_credential_source_kinds: BTreeMap<String, BTreeSet<CredentialSourceKind>>,
    required_provider_capabilities: BTreeMap<String, ProviderCapabilities>,
    required_model_capabilities: BTreeMap<String, ProviderCapabilities>,
}

impl ProviderPolicy {
    pub fn allow_all() -> Self {
        Self {
            allowed_providers: None,
            allowed_models: BTreeMap::new(),
            allowed_credential_kinds: BTreeMap::new(),
            allowed_credential_source_kinds: BTreeMap::new(),
            required_provider_capabilities: BTreeMap::new(),
            required_model_capabilities: BTreeMap::new(),
        }
    }

    pub fn enterprise_direct() -> Self {
        Self::allow_only(ENTERPRISE_DIRECT_PROVIDERS.iter().copied())
    }

    pub fn enterprise_direct_agentic() -> Self {
        let mut policy = Self::enterprise_direct();
        for provider in ENTERPRISE_DIRECT_PROVIDERS {
            policy
                .required_model_capabilities
                .insert((*provider).to_string(), ProviderCapabilities::agentic());
        }
        policy
    }

    pub fn allow_only<I, S>(providers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_providers: Some(providers.into_iter().map(Into::into).collect()),
            allowed_models: BTreeMap::new(),
            allowed_credential_kinds: BTreeMap::new(),
            allowed_credential_source_kinds: BTreeMap::new(),
            required_provider_capabilities: BTreeMap::new(),
            required_model_capabilities: BTreeMap::new(),
        }
    }

    pub fn with_allowed_models<I, S>(mut self, provider: impl Into<String>, models: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_models.insert(
            provider.into(),
            models.into_iter().map(Into::into).collect(),
        );
        self
    }

    pub fn with_allowed_credential_kinds<I>(mut self, provider: impl Into<String>, kinds: I) -> Self
    where
        I: IntoIterator<Item = CredentialKind>,
    {
        self.allowed_credential_kinds
            .insert(provider.into(), kinds.into_iter().collect());
        self
    }

    pub fn with_allowed_credential_source_kinds<I>(
        mut self,
        provider: impl Into<String>,
        kinds: I,
    ) -> Self
    where
        I: IntoIterator<Item = CredentialSourceKind>,
    {
        self.allowed_credential_source_kinds
            .insert(provider.into(), kinds.into_iter().collect());
        self
    }

    pub fn with_required_provider_capabilities(
        mut self,
        provider: impl Into<String>,
        capabilities: ProviderCapabilities,
    ) -> Self {
        self.required_provider_capabilities
            .insert(provider.into(), capabilities);
        self
    }

    pub fn with_required_model_capabilities(
        mut self,
        provider: impl Into<String>,
        capabilities: ProviderCapabilities,
    ) -> Self {
        self.required_model_capabilities
            .insert(provider.into(), capabilities);
        self
    }

    pub(crate) fn canonicalize(self, registry: &ProviderRegistry) -> Self {
        let allowed_providers = self.allowed_providers.map(|allowed| {
            allowed
                .into_iter()
                .map(|provider| canonical_provider(registry, provider))
                .collect()
        });
        let allowed_models = self
            .allowed_models
            .into_iter()
            .map(|(provider, models)| (canonical_provider(registry, provider), models))
            .collect();
        let allowed_credential_kinds = self
            .allowed_credential_kinds
            .into_iter()
            .map(|(provider, kinds)| (canonical_provider(registry, provider), kinds))
            .collect();
        let allowed_credential_source_kinds = self
            .allowed_credential_source_kinds
            .into_iter()
            .map(|(provider, kinds)| (canonical_provider(registry, provider), kinds))
            .collect();
        let required_provider_capabilities = self
            .required_provider_capabilities
            .into_iter()
            .map(|(provider, capabilities)| (canonical_provider(registry, provider), capabilities))
            .collect();
        let required_model_capabilities = self
            .required_model_capabilities
            .into_iter()
            .map(|(provider, capabilities)| (canonical_provider(registry, provider), capabilities))
            .collect();

        Self {
            allowed_providers,
            allowed_models,
            allowed_credential_kinds,
            allowed_credential_source_kinds,
            required_provider_capabilities,
            required_model_capabilities,
        }
    }

    pub(crate) fn check_provider(&self, provider: &str) -> Result<(), ProviderError> {
        if let Some(allowed) = &self.allowed_providers
            && !allowed.contains(provider)
        {
            return Err(ProviderError::PolicyDenied {
                provider: provider.to_string(),
                reason: "provider is not allowed".to_string(),
            });
        }

        Ok(())
    }

    pub(crate) fn check_provider_capabilities(
        &self,
        provider: &str,
        capabilities: &ProviderCapabilities,
    ) -> Result<(), ProviderError> {
        if let Some(required) = self.required_provider_capabilities.get(provider)
            && !capabilities_satisfy(capabilities, required)
        {
            return Err(ProviderError::PolicyDenied {
                provider: provider.to_string(),
                reason: "provider capabilities do not satisfy policy".to_string(),
            });
        }

        Ok(())
    }

    pub(crate) fn check_credential_kind(
        &self,
        provider: &str,
        credential_kind: Option<CredentialKind>,
    ) -> Result<(), ProviderError> {
        if let Some(allowed) = self.allowed_credential_kinds.get(provider) {
            let Some(credential_kind) = credential_kind else {
                return Err(ProviderError::PolicyDenied {
                    provider: provider.to_string(),
                    reason: "credential kind is not allowed: missing".to_string(),
                });
            };

            if !allowed.contains(&credential_kind) {
                return Err(ProviderError::PolicyDenied {
                    provider: provider.to_string(),
                    reason: format!("credential kind is not allowed: {credential_kind:?}"),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn check_credential_source_kind(
        &self,
        provider: &str,
        credential_source_kind: Option<CredentialSourceKind>,
    ) -> Result<(), ProviderError> {
        if let Some(allowed) = self.allowed_credential_source_kinds.get(provider) {
            let Some(credential_source_kind) = credential_source_kind else {
                return Err(ProviderError::PolicyDenied {
                    provider: provider.to_string(),
                    reason: "credential source kind is not allowed: missing".to_string(),
                });
            };

            if !allowed.contains(&credential_source_kind) {
                return Err(ProviderError::PolicyDenied {
                    provider: provider.to_string(),
                    reason: format!(
                        "credential source kind is not allowed: {credential_source_kind:?}"
                    ),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn check_model(&self, provider: &str, model: &str) -> Result<(), ProviderError> {
        if let Some(allowed) = self.allowed_models.get(provider)
            && !allowed.contains(model)
        {
            return Err(ProviderError::PolicyDenied {
                provider: provider.to_string(),
                reason: format!("model is not allowed: {model}"),
            });
        }

        Ok(())
    }

    pub(crate) fn filter_models(
        &self,
        provider: &str,
        models: Vec<ProviderModel>,
    ) -> Vec<ProviderModel> {
        let models = if let Some(allowed) = self.allowed_models.get(provider) {
            models
                .into_iter()
                .filter(|model| allowed.contains(&model.id))
                .collect()
        } else {
            models
        };

        if let Some(required) = self.required_model_capabilities.get(provider) {
            models
                .into_iter()
                .filter(|model| capabilities_satisfy(&model.capability_flags, required))
                .collect()
        } else {
            models
        }
    }

    pub(crate) fn catalog_policy(&self, provider: &str) -> ProviderModelCatalogPolicy {
        ProviderModelCatalogPolicy {
            allowed_models: self
                .allowed_models
                .get(provider)
                .map(|models| models.iter().cloned().collect()),
            allowed_credential_kinds: self
                .allowed_credential_kinds
                .get(provider)
                .map(|kinds| kinds.iter().copied().collect()),
            allowed_credential_source_kinds: self
                .allowed_credential_source_kinds
                .get(provider)
                .map(|kinds| kinds.iter().copied().collect()),
            required_provider_capabilities: self
                .required_provider_capabilities
                .get(provider)
                .copied()
                .unwrap_or_default(),
            required_capabilities: self
                .required_model_capabilities
                .get(provider)
                .copied()
                .unwrap_or_default(),
        }
    }

    pub(crate) fn route_policy(&self, provider: &str) -> ProviderRoutePolicy {
        ProviderRoutePolicy {
            allowed_models: self
                .allowed_models
                .get(provider)
                .map(|models| models.iter().cloned().collect()),
            allowed_credential_kinds: self
                .allowed_credential_kinds
                .get(provider)
                .map(|kinds| kinds.iter().copied().collect()),
            allowed_credential_source_kinds: self
                .allowed_credential_source_kinds
                .get(provider)
                .map(|kinds| kinds.iter().copied().collect()),
            required_provider_capabilities: self
                .required_provider_capabilities
                .get(provider)
                .copied()
                .unwrap_or_default(),
            required_model_capabilities: self
                .required_model_capabilities
                .get(provider)
                .copied()
                .unwrap_or_default(),
        }
    }
}

fn canonical_provider(registry: &ProviderRegistry, provider: String) -> String {
    registry
        .resolve(&provider)
        .map(|profile| profile.id.to_string())
        .unwrap_or(provider)
}

fn capabilities_satisfy(model: &ProviderCapabilities, required: &ProviderCapabilities) -> bool {
    (!required.tools || model.tools)
        && (!required.streaming || model.streaming)
        && (!required.vision || model.vision)
        && (!required.reasoning || model.reasoning)
}
