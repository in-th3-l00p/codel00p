use crate::UsagePricing;

/// Published provider/model pricing rows for organization-managed clients.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderPricingCatalog {
    #[serde(default)]
    pub entries: Vec<ProviderModelPricing>,
}

impl ProviderPricingCatalog {
    pub fn new(entries: impl IntoIterator<Item = ProviderModelPricing>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}

/// One provider/model pricing row in a published pricing catalog.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderModelPricing {
    pub provider: String,
    pub model: String,
    pub pricing: UsagePricing,
}

impl ProviderModelPricing {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        pricing: UsagePricing,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            pricing,
        }
    }
}
