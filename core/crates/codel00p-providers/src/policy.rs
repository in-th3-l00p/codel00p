use std::collections::{BTreeMap, BTreeSet};

use crate::{ProviderError, ProviderModel, ProviderRegistry};

/// Provider policy enforced during route resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPolicy {
    allowed_providers: Option<BTreeSet<String>>,
    allowed_models: BTreeMap<String, BTreeSet<String>>,
}

impl ProviderPolicy {
    pub fn allow_all() -> Self {
        Self {
            allowed_providers: None,
            allowed_models: BTreeMap::new(),
        }
    }

    pub fn allow_only<I, S>(providers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_providers: Some(providers.into_iter().map(Into::into).collect()),
            allowed_models: BTreeMap::new(),
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

        Self {
            allowed_providers,
            allowed_models,
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
        let Some(allowed) = self.allowed_models.get(provider) else {
            return models;
        };

        models
            .into_iter()
            .filter(|model| allowed.contains(&model.id))
            .collect()
    }
}

fn canonical_provider(registry: &ProviderRegistry, provider: String) -> String {
    registry
        .resolve(&provider)
        .map(|profile| profile.id.to_string())
        .unwrap_or(provider)
}
