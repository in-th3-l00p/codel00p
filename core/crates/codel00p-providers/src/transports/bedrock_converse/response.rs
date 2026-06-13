//! Bedrock Converse response normalization into provider-neutral responses.

use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BedrockConverseResponse {
    output: Option<BedrockOutput>,
    stop_reason: Option<String>,
    usage: Option<BedrockUsage>,
}

impl BedrockConverseResponse {
    pub(super) fn normalize(self, _provider: &str) -> Result<InferenceResponse, ProviderError> {
        let mut text_blocks = Vec::new();
        let mut tool_calls = Vec::new();

        if let Some(output) = self.output {
            for block in output.message.content {
                match block {
                    BedrockResponseContentBlock::Text { text } => {
                        if !text.is_empty() {
                            text_blocks.push(text);
                        }
                    }
                    BedrockResponseContentBlock::ToolUse { tool_use } => {
                        tool_calls.push(tool_use.normalize());
                    }
                    BedrockResponseContentBlock::Other { .. } => {}
                }
            }
        }

        Ok(InferenceResponse {
            content: if text_blocks.is_empty() {
                None
            } else {
                Some(text_blocks.join("\n"))
            },
            tool_calls,
            finish_reason: self.stop_reason,
            reasoning: None,
            usage: self.usage.map(BedrockUsage::normalize),
            cost: None,
            provider_data: Default::default(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct BedrockOutput {
    message: BedrockResponseMessage,
}

#[derive(Debug, Deserialize)]
struct BedrockResponseMessage {
    #[allow(dead_code)]
    role: Option<String>,
    #[serde(default)]
    content: Vec<BedrockResponseContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BedrockResponseContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: BedrockResponseToolUse,
    },
    Other {},
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BedrockResponseToolUse {
    tool_use_id: String,
    name: String,
    input: Value,
}

impl BedrockResponseToolUse {
    fn normalize(self) -> ToolCall {
        ToolCall {
            id: Some(self.tool_use_id),
            name: self.name,
            arguments: self.input,
            provider_data: Default::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BedrockUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_write_input_tokens: Option<u64>,
}

impl BedrockUsage {
    fn normalize(self) -> Usage {
        let input_total = self.input_tokens.unwrap_or_default();
        let cache_read = self.cache_read_input_tokens.unwrap_or_default();
        Usage {
            input_tokens: input_total.saturating_sub(cache_read),
            output_tokens: self.output_tokens.unwrap_or_default(),
            cache_read_tokens: cache_read,
            cache_write_tokens: self.cache_write_input_tokens.unwrap_or_default(),
            reasoning_tokens: 0,
        }
    }
}
