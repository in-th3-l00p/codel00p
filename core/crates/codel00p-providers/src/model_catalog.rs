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
    pub description: Option<String>,
    pub owned_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "ProviderModelAnnotations::is_empty")]
    pub annotations: ProviderModelAnnotations,
    #[serde(default, skip_serializing_if = "ProviderModelLimits::is_empty")]
    pub limits: ProviderModelLimits,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_data: BTreeMap<String, Value>,
}

/// Normalized provider-specific model catalog annotations.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderModelAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl ProviderModelAnnotations {
    pub fn is_empty(&self) -> bool {
        self.registry.is_none()
            && self.html_url.is_none()
            && self.version.is_none()
            && self.rate_limit_tier.is_none()
            && self.tags.is_empty()
    }
}

/// Normalized model catalog token limits.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderModelLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

impl ProviderModelLimits {
    pub fn is_empty(&self) -> bool {
        self.max_input_tokens.is_none() && self.max_output_tokens.is_none()
    }

    fn from_catalog_values(limits: Option<&Value>, context_length: Option<&Value>) -> Self {
        Self {
            max_input_tokens: object_u64(limits, "max_input_tokens")
                .or_else(|| value_u64(context_length)),
            max_output_tokens: object_u64(limits, "max_output_tokens"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum ModelCatalogWireResponse {
    OpenAiCompatible(ModelCatalogWireList),
    GitHubModels(Vec<ModelCatalogWireModel>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelCatalogWireList {
    data: Vec<ModelCatalogWireModel>,
}

impl ModelCatalogWireResponse {
    pub(crate) fn into_models(self) -> Vec<ProviderModel> {
        match self {
            Self::OpenAiCompatible(list) => list.data,
            Self::GitHubModels(models) => models,
        }
        .into_iter()
        .map(ModelCatalogWireModel::into_model)
        .collect()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelCatalogWireModel {
    id: String,
    name: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    summary: Option<String>,
    owned_by: Option<String>,
    publisher: Option<String>,
    registry: Option<String>,
    html_url: Option<String>,
    version: Option<String>,
    rate_limit_tier: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    limits: Option<Value>,
    #[serde(default)]
    supported_input_modalities: Vec<String>,
    #[serde(default)]
    supported_output_modalities: Vec<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl ModelCatalogWireModel {
    fn into_model(self) -> ProviderModel {
        let mut provider_data = self.extra;
        let limits = ProviderModelLimits::from_catalog_values(
            self.limits.as_ref(),
            provider_data.get("context_length"),
        );
        if let Some(publisher) = self.publisher.as_ref() {
            provider_data.insert("publisher".to_string(), Value::String(publisher.clone()));
        }
        if let Some(description) = self.description.as_ref() {
            provider_data.insert(
                "description".to_string(),
                Value::String(description.clone()),
            );
        }
        if let Some(summary) = self.summary.as_ref() {
            provider_data.insert("summary".to_string(), Value::String(summary.clone()));
        }
        if let Some(registry) = self.registry.as_ref() {
            provider_data.insert("registry".to_string(), Value::String(registry.clone()));
        }
        if let Some(html_url) = self.html_url.as_ref() {
            provider_data.insert("html_url".to_string(), Value::String(html_url.clone()));
        }
        if let Some(version) = self.version.as_ref() {
            provider_data.insert("version".to_string(), Value::String(version.clone()));
        }
        if let Some(rate_limit_tier) = self.rate_limit_tier.as_ref() {
            provider_data.insert(
                "rate_limit_tier".to_string(),
                Value::String(rate_limit_tier.clone()),
            );
        }
        if !self.tags.is_empty() {
            provider_data.insert("tags".to_string(), string_values(&self.tags));
        }
        if !self.capabilities.is_empty() {
            provider_data.insert(
                "capabilities".to_string(),
                string_values(&self.capabilities),
            );
        }
        if let Some(limits) = self.limits {
            provider_data.insert("limits".to_string(), limits);
        }
        if !self.supported_input_modalities.is_empty() {
            provider_data.insert(
                "supported_input_modalities".to_string(),
                string_values(&self.supported_input_modalities),
            );
        }
        if !self.supported_output_modalities.is_empty() {
            provider_data.insert(
                "supported_output_modalities".to_string(),
                string_values(&self.supported_output_modalities),
            );
        }

        ProviderModel {
            id: self.id,
            display_name: self.display_name.or(self.name),
            description: self.description.or(self.summary),
            owned_by: self.owned_by.or(self.publisher),
            capabilities: self.capabilities,
            input_modalities: self.supported_input_modalities,
            output_modalities: self.supported_output_modalities,
            annotations: ProviderModelAnnotations {
                registry: self.registry,
                html_url: self.html_url,
                version: self.version,
                rate_limit_tier: self.rate_limit_tier,
                tags: self.tags,
            },
            limits,
            provider_data,
        }
    }
}

fn object_u64(object: Option<&Value>, key: &str) -> Option<u64> {
    object?.get(key).and_then(|value| value_u64(Some(value)))
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn string_values(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}
