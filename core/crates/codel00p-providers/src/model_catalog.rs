use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// Request for listing models from a provider catalog endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogRequest {
    pub provider: String,
    pub base_url: Option<String>,
    pub models_url: Option<String>,
}

impl ModelCatalogRequest {
    pub fn builder(provider: impl Into<String>) -> ModelCatalogRequestBuilder {
        ModelCatalogRequestBuilder {
            request: Self {
                provider: provider.into(),
                base_url: None,
                models_url: None,
            },
        }
    }
}

/// Builder for [`ModelCatalogRequest`].
#[derive(Debug, Clone)]
pub struct ModelCatalogRequestBuilder {
    request: ModelCatalogRequest,
}

impl ModelCatalogRequestBuilder {
    pub fn base_url(mut self, value: impl Into<String>) -> Self {
        self.request.base_url = Some(value.into());
        self
    }

    pub fn models_url(mut self, value: impl Into<String>) -> Self {
        self.request.models_url = Some(value.into());
        self
    }

    pub fn build(self) -> ModelCatalogRequest {
        self.request
    }
}

/// Normalized provider model descriptor.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderModel {
    pub id: String,
    pub display_name: Option<String>,
    pub owned_by: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_data: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelCatalogWireResponse {
    data: Vec<ModelCatalogWireModel>,
}

impl ModelCatalogWireResponse {
    pub(crate) fn into_models(self) -> Vec<ProviderModel> {
        self.data
            .into_iter()
            .map(ModelCatalogWireModel::into_model)
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct ModelCatalogWireModel {
    id: String,
    name: Option<String>,
    display_name: Option<String>,
    owned_by: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl ModelCatalogWireModel {
    fn into_model(self) -> ProviderModel {
        ProviderModel {
            id: self.id,
            display_name: self.display_name.or(self.name),
            owned_by: self.owned_by,
            provider_data: self.extra,
        }
    }
}
