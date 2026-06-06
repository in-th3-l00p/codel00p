use std::collections::BTreeSet;

use crate::{ProviderError, ProviderRegistry};

/// Provider policy enforced during route resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPolicy {
    allowed_providers: Option<BTreeSet<String>>,
}

impl ProviderPolicy {
    pub fn allow_all() -> Self {
        Self {
            allowed_providers: None,
        }
    }

    pub fn allow_only<I, S>(providers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_providers: Some(providers.into_iter().map(Into::into).collect()),
        }
    }

    pub(crate) fn canonicalize(self, registry: &ProviderRegistry) -> Self {
        let Some(allowed) = self.allowed_providers else {
            return self;
        };

        Self {
            allowed_providers: Some(
                allowed
                    .into_iter()
                    .map(|provider| {
                        registry
                            .resolve(&provider)
                            .map(|profile| profile.id.to_string())
                            .unwrap_or(provider)
                    })
                    .collect(),
            ),
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
}
