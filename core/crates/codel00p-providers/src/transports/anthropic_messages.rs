use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole, ProviderError,
    ToolCall, ToolDefinition, Usage,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 1024;

pub(crate) struct AnthropicMessagesTransport {
    http: reqwest::Client,
}

impl AnthropicMessagesTransport {
    pub(crate) fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    pub(crate) async fn complete(
        &self,
        provider: &str,
        base_url: &str,
        credential: &Credential,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, ProviderError> {
        let Credential::ApiKey(api_key) = credential else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let wire_request = AnthropicMessagesRequest::from_request(provider, request)?;
        let response = self
            .http
            .post(messages_url(base_url))
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&wire_request)
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Http {
                provider: provider.to_string(),
                message: format!("status {status}: {body}"),
            });
        }

        let wire_response =
            response
                .json::<AnthropicMessagesResponse>()
                .await
                .map_err(|error| ProviderError::InvalidResponse {
                    provider: provider.to_string(),
                    message: error.to_string(),
                })?;

        wire_response.normalize(provider)
    }
}

fn messages_url(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1") {
        format!("{base_url}/messages")
    } else {
        format!("{base_url}/v1/messages")
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
}

impl AnthropicMessagesRequest {
    fn from_request(provider: &str, request: InferenceRequest) -> Result<Self, ProviderError> {
        let mut system_messages = Vec::new();
        let mut messages = Vec::new();

        for message in request.messages {
            match message.role {
                MessageRole::System | MessageRole::Developer => {
                    if let Some(content) = message.content
                        && !content.is_empty()
                    {
                        system_messages.push(content);
                    }
                }
                _ => messages.push(AnthropicMessage::from_chat_message(provider, message)?),
            }
        }

        Ok(Self {
            model: request.model,
            max_tokens: request.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages,
            system: join_system_messages(system_messages),
            temperature: request.temperature,
            tools: request.tools.into_iter().map(AnthropicTool::from).collect(),
        })
    }
}

fn join_system_messages(messages: Vec<String>) -> Option<String> {
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("\n\n"))
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: AnthropicContent,
}

impl AnthropicMessage {
    fn from_chat_message(provider: &str, message: ChatMessage) -> Result<Self, ProviderError> {
        match message.role {
            MessageRole::User => Ok(Self {
                role: "user",
                content: AnthropicContent::Text(message.content.unwrap_or_default()),
            }),
            MessageRole::Assistant => Ok(Self {
                role: "assistant",
                content: assistant_content(message.content, message.tool_calls),
            }),
            MessageRole::Tool => Ok(Self {
                role: "user",
                content: AnthropicContent::Blocks(vec![AnthropicContentBlock::ToolResult {
                    tool_use_id: message.tool_call_id.ok_or_else(|| {
                        ProviderError::InvalidResponse {
                            provider: provider.to_string(),
                            message: "tool result message is missing tool_call_id".to_string(),
                        }
                    })?,
                    content: message.content.unwrap_or_default(),
                }]),
            }),
            MessageRole::System | MessageRole::Developer => unreachable!("handled by caller"),
        }
    }
}

fn assistant_content(content: Option<String>, tool_calls: Vec<ToolCall>) -> AnthropicContent {
    if tool_calls.is_empty() {
        return AnthropicContent::Text(content.unwrap_or_default());
    }

    let mut blocks = Vec::new();
    if let Some(content) = content
        && !content.is_empty()
    {
        blocks.push(AnthropicContentBlock::Text { text: content });
    }
    blocks.extend(tool_calls.into_iter().map(AnthropicContentBlock::from));
    AnthropicContent::Blocks(blocks)
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

impl From<ToolCall> for AnthropicContentBlock {
    fn from(tool_call: ToolCall) -> Self {
        Self::ToolUse {
            id: tool_call.id.unwrap_or_else(|| tool_call.name.clone()),
            name: tool_call.name,
            input: tool_call.arguments,
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

impl From<ToolDefinition> for AnthropicTool {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            name: tool.name,
            description: tool.description,
            input_schema: tool.parameters,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    id: Option<String>,
    model: Option<String>,
    #[serde(default)]
    content: Vec<AnthropicResponseContentBlock>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

impl AnthropicMessagesResponse {
    fn normalize(self, provider: &str) -> Result<InferenceResponse, ProviderError> {
        let mut text_blocks = Vec::new();
        let mut tool_calls = Vec::new();

        for block in self.content {
            match block.kind.as_str() {
                "text" => {
                    if let Some(text) = block.text
                        && !text.is_empty()
                    {
                        text_blocks.push(text);
                    }
                }
                "tool_use" => {
                    tool_calls.push(block.normalize_tool_use(provider)?);
                }
                _ => {}
            }
        }

        let mut provider_data = BTreeMap::new();
        if let Some(id) = self.id {
            provider_data.insert("id".to_string(), Value::String(id));
        }
        if let Some(model) = self.model {
            provider_data.insert("model".to_string(), Value::String(model));
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
            usage: self.usage.map(AnthropicUsage::normalize),
            cost: None,
            provider_data,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicResponseContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<Value>,
}

impl AnthropicResponseContentBlock {
    fn normalize_tool_use(self, provider: &str) -> Result<ToolCall, ProviderError> {
        let id = self.id.ok_or_else(|| ProviderError::InvalidResponse {
            provider: provider.to_string(),
            message: "tool_use block is missing id".to_string(),
        })?;
        let name = self.name.ok_or_else(|| ProviderError::InvalidResponse {
            provider: provider.to_string(),
            message: "tool_use block is missing name".to_string(),
        })?;

        Ok(ToolCall {
            id: Some(id),
            name,
            arguments: self.input.unwrap_or(Value::Object(Default::default())),
            provider_data: Default::default(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

impl AnthropicUsage {
    fn normalize(self) -> Usage {
        Usage {
            input_tokens: self.input_tokens.unwrap_or_default(),
            output_tokens: self.output_tokens.unwrap_or_default(),
            cache_read_tokens: self.cache_read_input_tokens.unwrap_or_default(),
            cache_write_tokens: self.cache_creation_input_tokens.unwrap_or_default(),
            reasoning_tokens: 0,
        }
    }
}
