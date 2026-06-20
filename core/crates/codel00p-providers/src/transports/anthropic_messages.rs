use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole, ProviderError,
    TokenSink, ToolCall, ToolDefinition, Usage, transports::sse::stream_sse,
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
            let retry_after = super::retry_after_seconds(response.headers());
            let body = response.text().await.unwrap_or_default();
            return Err(super::http_status_error(
                provider,
                status,
                &body,
                retry_after,
            ));
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

    pub(crate) async fn complete_streaming(
        &self,
        provider: &str,
        base_url: &str,
        credential: &Credential,
        request: InferenceRequest,
        sink: &dyn TokenSink,
    ) -> Result<InferenceResponse, ProviderError> {
        let Credential::ApiKey(api_key) = credential else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let wire_request = AnthropicMessagesRequest::from_request(provider, request)?.streaming();
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

        let mut accumulator = AnthropicStreamAccumulator::default();
        stream_sse(provider, response, |event| {
            accumulator.ingest(provider, event, sink)?;
            Ok(false)
        })
        .await?;
        Ok(accumulator.finish())
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
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

impl AnthropicMessagesRequest {
    fn streaming(mut self) -> Self {
        self.stream = true;
        self
    }

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

        let tool_choice = (!request.tools.is_empty())
            .then(|| {
                request
                    .tool_choice
                    .as_ref()
                    .map(|choice| choice.anthropic_value())
            })
            .flatten();
        Ok(Self {
            model: request.model,
            max_tokens: request.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            messages,
            system: join_system_messages(system_messages),
            temperature: request.temperature,
            tools: request.tools.into_iter().map(AnthropicTool::from).collect(),
            tool_choice,
            stream: false,
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

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    kind: String,
    message: Option<AnthropicStreamMessage>,
    index: Option<usize>,
    content_block: Option<AnthropicStreamContentBlock>,
    delta: Option<AnthropicStreamDelta>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamMessage {
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamContentBlock {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamDelta {
    text: Option<String>,
    partial_json: Option<String>,
    stop_reason: Option<String>,
}

#[derive(Default)]
struct AnthropicStreamAccumulator {
    content: String,
    has_content: bool,
    tool_calls: BTreeMap<usize, AnthropicStreamToolCall>,
    finish_reason: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    saw_usage: bool,
}

#[derive(Default)]
struct AnthropicStreamToolCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

impl AnthropicStreamAccumulator {
    fn ingest(
        &mut self,
        provider: &str,
        payload: &str,
        sink: &dyn TokenSink,
    ) -> Result<(), ProviderError> {
        let event: AnthropicStreamEvent =
            serde_json::from_str(payload).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        match event.kind.as_str() {
            "message_start" => {
                if let Some(usage) = event.message.and_then(|message| message.usage) {
                    self.merge_usage(usage);
                }
            }
            "content_block_start" => {
                if let (Some(index), Some(block)) = (event.index, event.content_block)
                    && block.kind == "tool_use"
                {
                    let name = block.name.unwrap_or_default();
                    // Surface the opening of a tool call (id + name, no args yet)
                    // so callers can show it being assembled live. The final
                    // assembled call (built in `finish`) is unchanged.
                    sink.on_tool_call_delta(index, block.id.as_deref(), Some(&name), "");
                    self.tool_calls.insert(
                        index,
                        AnthropicStreamToolCall {
                            id: block.id,
                            name,
                            arguments: String::new(),
                        },
                    );
                }
            }
            "content_block_delta" => {
                if let Some(delta) = event.delta {
                    if let Some(text) = delta.text
                        && !text.is_empty()
                    {
                        self.has_content = true;
                        self.content.push_str(&text);
                        sink.on_token(&text);
                    }
                    if let (Some(index), Some(partial)) = (event.index, delta.partial_json)
                        && let Some(slot) = self.tool_calls.get_mut(&index)
                    {
                        slot.arguments.push_str(&partial);
                        sink.on_tool_call_delta(index, slot.id.as_deref(), None, &partial);
                    }
                }
            }
            "message_delta" => {
                if let Some(stop_reason) = event.delta.and_then(|delta| delta.stop_reason) {
                    self.finish_reason = Some(stop_reason);
                }
                if let Some(usage) = event.usage {
                    self.merge_usage(usage);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn merge_usage(&mut self, usage: AnthropicUsage) {
        self.saw_usage = true;
        if let Some(input) = usage.input_tokens {
            self.input_tokens = input;
        }
        if let Some(output) = usage.output_tokens {
            self.output_tokens = output;
        }
        if let Some(cache_read) = usage.cache_read_input_tokens {
            self.cache_read_tokens = cache_read;
        }
        if let Some(cache_write) = usage.cache_creation_input_tokens {
            self.cache_write_tokens = cache_write;
        }
    }

    fn finish(self) -> InferenceResponse {
        let tool_calls = self
            .tool_calls
            .into_values()
            .filter(|call| !call.name.is_empty())
            .map(|call| ToolCall {
                id: call.id,
                name: call.name,
                arguments: if call.arguments.is_empty() {
                    Value::Object(Default::default())
                } else {
                    serde_json::from_str(&call.arguments).unwrap_or(Value::String(call.arguments))
                },
                provider_data: Default::default(),
            })
            .collect();

        InferenceResponse {
            content: self.has_content.then_some(self.content),
            tool_calls,
            finish_reason: self.finish_reason,
            reasoning: None,
            usage: self.saw_usage.then_some(Usage {
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
                cache_read_tokens: self.cache_read_tokens,
                cache_write_tokens: self.cache_write_tokens,
                reasoning_tokens: 0,
            }),
            cost: None,
            provider_data: Default::default(),
        }
    }
}
