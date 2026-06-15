//! Builder and stored configuration types for InferenceClient.

use super::*;

impl InferenceClient {
    pub fn builder() -> InferenceClientBuilder {
        InferenceClientBuilder {
            registry: default_registry(),
            credentials: BTreeMap::new(),
            policy: ProviderPolicy::allow_all(),
            model_pricing: BTreeMap::new(),
            provider_proxies: BTreeMap::new(),
            retry: RetryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ProviderProxyRoute {
    pub(super) base_url: String,
    pub(super) credential: Credential,
}

#[derive(Debug, Clone)]
pub(super) struct StoredUsagePricing {
    pub(super) pricing: UsagePricing,
    pub(super) source: String,
}

/// Builder for [`InferenceClient`].
#[derive(Debug, Clone)]
pub struct InferenceClientBuilder {
    registry: ProviderRegistry,
    credentials: BTreeMap<String, ResolvedProviderCredential>,
    policy: ProviderPolicy,
    model_pricing: BTreeMap<String, BTreeMap<String, StoredUsagePricing>>,
    provider_proxies: BTreeMap<String, ProviderProxyRoute>,
    retry: RetryPolicy,
}

impl InferenceClientBuilder {
    pub fn registry(mut self, registry: ProviderRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Overrides the per-route retry policy (default: two retries with backoff).
    pub fn retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn credential(mut self, provider: impl Into<String>, credential: Credential) -> Self {
        self.credentials.insert(
            provider.into(),
            ResolvedProviderCredential {
                credential,
                source: "configured".to_string(),
                source_kind: CredentialSourceKind::Configured,
            },
        );
        self
    }

    pub fn organization_credential(
        mut self,
        provider: impl Into<String>,
        credential: Credential,
        organization_ref: impl Into<String>,
    ) -> Self {
        self.credentials.insert(
            provider.into(),
            ResolvedProviderCredential {
                credential,
                source: format!("organization:{}", organization_ref.into()),
                source_kind: CredentialSourceKind::Organization,
            },
        );
        self
    }

    pub fn managed_identity_credential(
        mut self,
        provider: impl Into<String>,
        credential: Credential,
        identity_ref: impl Into<String>,
    ) -> Self {
        self.credentials.insert(
            provider.into(),
            ResolvedProviderCredential {
                credential,
                source: format!("managed_identity:{}", identity_ref.into()),
                source_kind: CredentialSourceKind::ManagedIdentity,
            },
        );
        self
    }

    pub fn managed_identity_credential_from_resolver<R>(
        self,
        provider: impl Into<String>,
        identity_ref: impl Into<String>,
        resolver: &R,
    ) -> Result<Self, ProviderError>
    where
        R: ManagedIdentityCredentialResolver,
    {
        let provider = provider.into();
        let identity_ref = identity_ref.into();
        let credential = resolver.resolve(ManagedIdentityCredentialRequest::new(
            &provider,
            &identity_ref,
        ))?;
        Ok(self.managed_identity_credential(provider, credential, identity_ref))
    }

    pub async fn azure_managed_identity_credential_from_resolver(
        self,
        provider: impl Into<String>,
        identity_ref: impl Into<String>,
        resolver: &AzureManagedIdentityCredentialResolver,
    ) -> Result<Self, ProviderError> {
        let provider = provider.into();
        let identity_ref = identity_ref.into();
        let credential = resolver.resolve(&provider, &identity_ref).await?;
        Ok(self.managed_identity_credential(provider, credential, identity_ref))
    }

    pub async fn aws_managed_identity_credential_from_resolver(
        self,
        provider: impl Into<String>,
        identity_ref: impl Into<String>,
        resolver: &AwsManagedIdentityCredentialResolver,
    ) -> Result<Self, ProviderError> {
        let provider = provider.into();
        let identity_ref = identity_ref.into();
        let credential = resolver.resolve(&provider, &identity_ref).await?;
        Ok(self.managed_identity_credential(provider, credential, identity_ref))
    }

    pub async fn gcp_managed_identity_credential_from_resolver(
        self,
        provider: impl Into<String>,
        identity_ref: impl Into<String>,
        resolver: &GcpManagedIdentityCredentialResolver,
    ) -> Result<Self, ProviderError> {
        let provider = provider.into();
        let identity_ref = identity_ref.into();
        let credential = resolver.resolve(&provider, &identity_ref).await?;
        Ok(self.managed_identity_credential(provider, credential, identity_ref))
    }

    pub fn credentials_from_env(mut self) -> Self {
        let loaded_credentials: Vec<_> = self
            .registry
            .profiles()
            .filter_map(|profile| {
                self.registry
                    .credential_from_env(profile.id)
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
            .insert(
                model.into(),
                StoredUsagePricing {
                    pricing,
                    source: "configured".to_string(),
                },
            );
        self
    }

    pub fn pricing_catalog(mut self, catalog: ProviderPricingCatalog) -> Self {
        let source = catalog.source.unwrap_or_else(|| "catalog".to_string());
        for entry in catalog.entries {
            self.model_pricing
                .entry(entry.provider)
                .or_default()
                .insert(
                    entry.model,
                    StoredUsagePricing {
                        pricing: entry.pricing,
                        source: source.clone(),
                    },
                );
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
            retry: self.retry,
        }
    }
}
