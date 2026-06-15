//! Bedrock Converse request serialization and endpoint construction.

use super::*;

pub(super) fn converse_url(base_url: &str, region: &str, model: &str) -> String {
    let model = urlencoding::encode(model);
    let base_url = bedrock_base_url(base_url, region);
    format!("{base_url}/model/{model}/converse")
}

pub(super) fn converse_stream_url(base_url: &str, region: &str, model: &str) -> String {
    let model = urlencoding::encode(model);
    let base_url = bedrock_base_url(base_url, region);
    format!("{base_url}/model/{model}/converse-stream")
}

fn bedrock_base_url(base_url: &str, region: &str) -> String {
    const DEFAULT_BEDROCK_BASE_URL: &str = "https://bedrock-runtime.us-east-1.amazonaws.com";
    if base_url.trim_end_matches('/') == DEFAULT_BEDROCK_BASE_URL && region != "us-east-1" {
        return format!("https://bedrock-runtime.{region}.amazonaws.com");
    }

    base_url.trim_end_matches('/').to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BedrockConverseRequest {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<BedrockSystemBlock>,
    messages: Vec<BedrockMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inference_config: Option<BedrockInferenceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<BedrockToolConfig>,
}

impl BedrockConverseRequest {
    pub(super) fn from_request(
        provider: &str,
        request: InferenceRequest,
    ) -> Result<Self, ProviderError> {
        let mut system = Vec::new();
        let mut messages = Vec::new();

        for message in request.messages {
            match message.role {
                MessageRole::System | MessageRole::Developer => {
                    if let Some(content) = message.content
                        && !content.is_empty()
                    {
                        system.push(BedrockSystemBlock { text: content });
                    }
                }
                _ => messages.push(BedrockMessage::from_chat_message(provider, message)?),
            }
        }

        Ok(Self {
            system,
            messages,
            inference_config: BedrockInferenceConfig::from_options(
                request.temperature,
                request.max_output_tokens,
            ),
            tool_config: BedrockToolConfig::from_tools(request.tools, request.tool_choice.as_ref()),
        })
    }
}

#[derive(Debug, Serialize)]
struct BedrockSystemBlock {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockInferenceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

impl BedrockInferenceConfig {
    fn from_options(temperature: Option<f32>, max_tokens: Option<u32>) -> Option<Self> {
        if temperature.is_none() && max_tokens.is_none() {
            None
        } else {
            Some(Self {
                temperature,
                max_tokens,
            })
        }
    }
}

#[derive(Debug, Serialize)]
struct BedrockMessage {
    role: &'static str,
    content: Vec<BedrockContentBlock>,
}

impl BedrockMessage {
    fn from_chat_message(provider: &str, message: ChatMessage) -> Result<Self, ProviderError> {
        match message.role {
            MessageRole::User => Ok(Self {
                role: "user",
                content: vec![BedrockContentBlock::Text {
                    text: message.content.unwrap_or_default(),
                }],
            }),
            MessageRole::Assistant => Ok(Self {
                role: "assistant",
                content: assistant_content(message.content, message.tool_calls),
            }),
            MessageRole::Tool => Ok(Self {
                role: "user",
                content: vec![BedrockContentBlock::ToolResult {
                    tool_result: BedrockToolResult {
                        tool_use_id: message.tool_call_id.ok_or_else(|| {
                            ProviderError::InvalidResponse {
                                provider: provider.to_string(),
                                message: "tool result message is missing tool_call_id".to_string(),
                            }
                        })?,
                        content: vec![BedrockToolResultContent::Text {
                            text: message.content.unwrap_or_default(),
                        }],
                    },
                }],
            }),
            MessageRole::System | MessageRole::Developer => unreachable!("handled by caller"),
        }
    }
}

fn assistant_content(
    content: Option<String>,
    tool_calls: Vec<ToolCall>,
) -> Vec<BedrockContentBlock> {
    let mut blocks = Vec::new();
    if let Some(content) = content
        && !content.is_empty()
    {
        blocks.push(BedrockContentBlock::Text { text: content });
    }
    blocks.extend(tool_calls.into_iter().map(BedrockContentBlock::from));
    blocks
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum BedrockContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: BedrockToolUse,
    },
    ToolResult {
        #[serde(rename = "toolResult")]
        tool_result: BedrockToolResult,
    },
}

impl From<ToolCall> for BedrockContentBlock {
    fn from(tool_call: ToolCall) -> Self {
        Self::ToolUse {
            tool_use: BedrockToolUse {
                tool_use_id: tool_call.id.unwrap_or_else(|| tool_call.name.clone()),
                name: tool_call.name,
                input: tool_call.arguments,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockToolUse {
    tool_use_id: String,
    name: String,
    input: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockToolResult {
    tool_use_id: String,
    content: Vec<BedrockToolResultContent>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum BedrockToolResultContent {
    Text { text: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockToolConfig {
    tools: Vec<BedrockTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

impl BedrockToolConfig {
    fn from_tools(tools: Vec<ToolDefinition>, choice: Option<&crate::ToolChoice>) -> Option<Self> {
        if tools.is_empty() {
            None
        } else {
            Some(Self {
                tools: tools.into_iter().map(BedrockTool::from).collect(),
                tool_choice: choice.and_then(|choice| choice.bedrock_value()),
            })
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockTool {
    tool_spec: BedrockToolSpec,
}

impl From<ToolDefinition> for BedrockTool {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            tool_spec: BedrockToolSpec {
                name: tool.name,
                description: tool.description,
                input_schema: BedrockToolInputSchema {
                    json: tool.parameters,
                },
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BedrockToolSpec {
    name: String,
    description: String,
    input_schema: BedrockToolInputSchema,
}

#[derive(Debug, Serialize)]
struct BedrockToolInputSchema {
    json: Value,
}
