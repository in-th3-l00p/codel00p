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

/// Provider-neutral inference response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InferenceResponse {
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub reasoning: Option<String>,
    pub usage: Option<Usage>,
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
