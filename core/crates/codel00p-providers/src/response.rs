use std::collections::BTreeMap;

use serde_json::Value;

/// Normalized tool call emitted by any provider.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_data: BTreeMap<String, Value>,
}

/// Normalized usage counters.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
}

impl Usage {
    pub fn estimate_cost(&self, pricing: &UsagePricing) -> UsageCostEstimate {
        let input_nanos =
            estimate_component_nanos(self.input_tokens, pricing.input_nanos_per_million_tokens);
        let output_nanos =
            estimate_component_nanos(self.output_tokens, pricing.output_nanos_per_million_tokens);
        let cache_read_nanos = estimate_component_nanos(
            self.cache_read_tokens,
            pricing.cache_read_nanos_per_million_tokens,
        );
        let cache_write_nanos = estimate_component_nanos(
            self.cache_write_tokens,
            pricing.cache_write_nanos_per_million_tokens,
        );
        let reasoning_nanos = estimate_component_nanos(
            self.reasoning_tokens,
            pricing.reasoning_nanos_per_million_tokens,
        );

        UsageCostEstimate {
            currency: pricing.currency.clone(),
            input_nanos,
            output_nanos,
            cache_read_nanos,
            cache_write_nanos,
            reasoning_nanos,
            total_nanos: input_nanos
                .saturating_add(output_nanos)
                .saturating_add(cache_read_nanos)
                .saturating_add(cache_write_nanos)
                .saturating_add(reasoning_nanos),
        }
    }
}

/// Explicit token pricing in nano currency units per million tokens.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsagePricing {
    pub currency: String,
    pub input_nanos_per_million_tokens: u64,
    pub output_nanos_per_million_tokens: u64,
    pub cache_read_nanos_per_million_tokens: u64,
    pub cache_write_nanos_per_million_tokens: u64,
    pub reasoning_nanos_per_million_tokens: u64,
}

impl UsagePricing {
    pub fn nanos_per_million_tokens(
        currency: impl Into<String>,
        input_nanos_per_million_tokens: u64,
        output_nanos_per_million_tokens: u64,
    ) -> Self {
        Self {
            currency: currency.into(),
            input_nanos_per_million_tokens,
            output_nanos_per_million_tokens,
            cache_read_nanos_per_million_tokens: 0,
            cache_write_nanos_per_million_tokens: 0,
            reasoning_nanos_per_million_tokens: 0,
        }
    }

    pub fn usd_nanos_per_million_tokens(
        input_nanos_per_million_tokens: u64,
        output_nanos_per_million_tokens: u64,
    ) -> Self {
        Self::nanos_per_million_tokens(
            "USD",
            input_nanos_per_million_tokens,
            output_nanos_per_million_tokens,
        )
    }

    pub fn with_cache_read_nanos_per_million_tokens(mut self, value: u64) -> Self {
        self.cache_read_nanos_per_million_tokens = value;
        self
    }

    pub fn with_cache_write_nanos_per_million_tokens(mut self, value: u64) -> Self {
        self.cache_write_nanos_per_million_tokens = value;
        self
    }

    pub fn with_reasoning_nanos_per_million_tokens(mut self, value: u64) -> Self {
        self.reasoning_nanos_per_million_tokens = value;
        self
    }
}

/// Normalized usage cost estimate derived from explicit request pricing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageCostEstimate {
    pub currency: String,
    pub input_nanos: u64,
    pub output_nanos: u64,
    pub cache_read_nanos: u64,
    pub cache_write_nanos: u64,
    pub reasoning_nanos: u64,
    pub total_nanos: u64,
}

/// Provider-neutral inference response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InferenceResponse {
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub reasoning: Option<String>,
    pub usage: Option<Usage>,
    pub cost: Option<UsageCostEstimate>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_data: BTreeMap<String, Value>,
}

impl InferenceResponse {
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            tool_calls: Vec::new(),
            finish_reason: None,
            reasoning: None,
            usage: None,
            cost: None,
            provider_data: BTreeMap::new(),
        }
    }

    pub fn with_finish_reason(mut self, value: impl Into<String>) -> Self {
        self.finish_reason = Some(value.into());
        self
    }

    pub fn with_provider_data(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.provider_data.insert(key.into(), value.into());
        self
    }
}

fn estimate_component_nanos(tokens: u64, nanos_per_million_tokens: u64) -> u64 {
    let product = u128::from(tokens) * u128::from(nanos_per_million_tokens);
    if product == 0 {
        return 0;
    }
    product.div_ceil(1_000_000).min(u128::from(u64::MAX)) as u64
}
