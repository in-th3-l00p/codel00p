use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole,
    OutputTokenParameter, ProviderError, TokenSink, ToolCall, ToolDefinition, Usage,
    transports::sse::stream_sse,
};

pub(crate) struct ChatCompletionsTransport {
    http: reqwest::Client,
}

impl ChatCompletionsTransport {
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
        output_token_parameter: OutputTokenParameter,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, ProviderError> {
        let Credential::ApiKey(api_key) = credential else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let wire_request = ChatCompletionsRequest::from_request(request, output_token_parameter);
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(url)
            .bearer_auth(api_key)
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

        let wire_response = response
            .json::<ChatCompletionsResponse>()
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
        output_token_parameter: OutputTokenParameter,
        request: InferenceRequest,
        sink: &dyn TokenSink,
    ) -> Result<InferenceResponse, ProviderError> {
        let Credential::ApiKey(api_key) = credential else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let wire_request =
            ChatCompletionsRequest::from_request(request, output_token_parameter).streaming();
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(url)
            .bearer_auth(api_key)
            .json(&wire_request)
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        stream_chat_completions_response(provider, response, sink).await
    }
}

/// Decodes a streaming Chat Completions response: validates status, reads each
/// SSE `data:` chunk, emits content deltas to `sink`, and assembles the final
/// response. Shared by the OpenAI-compatible and Azure transports.
pub(crate) async fn stream_chat_completions_response(
    provider: &str,
    response: reqwest::Response,
    sink: &dyn TokenSink,
) -> Result<InferenceResponse, ProviderError> {
    let mut accumulator = StreamingAccumulator::default();
    stream_sse(provider, response, |event| {
        accumulator.ingest(provider, event, sink)?;
        Ok(false)
    })
    .await?;
    accumulator.finish(provider)
}

#[derive(Default)]
struct StreamingAccumulator {
    content: String,
    has_content: bool,
    finish_reason: Option<String>,
    usage: Option<Usage>,
    tool_calls: Vec<StreamingToolCall>,
}

#[derive(Default)]
struct StreamingToolCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

impl StreamingAccumulator {
    fn ingest(
        &mut self,
        provider: &str,
        payload: &str,
        sink: &dyn TokenSink,
    ) -> Result<(), ProviderError> {
        let chunk: ChatCompletionsStreamChunk =
            serde_json::from_str(payload).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        if let Some(usage) = chunk.usage {
            self.usage = Some(usage.normalize());
        }

        if let Some(choice) = chunk.choices.into_iter().next() {
            if let Some(reason) = choice.finish_reason {
                self.finish_reason = Some(reason);
            }
            if let Some(delta_content) = choice.delta.content
                && !delta_content.is_empty()
            {
                self.has_content = true;
                self.content.push_str(&delta_content);
                sink.on_token(&delta_content);
            }
            for delta in choice.delta.tool_calls {
                let index = delta.index.unwrap_or(0);
                if index >= self.tool_calls.len() {
                    self.tool_calls
                        .resize_with(index + 1, StreamingToolCall::default);
                }
                let slot = &mut self.tool_calls[index];
                if let Some(id) = delta.id {
                    slot.id = Some(id);
                }
                if let Some(function) = delta.function {
                    if let Some(name) = function.name {
                        slot.name.push_str(&name);
                    }
                    if let Some(arguments) = function.arguments {
                        slot.arguments.push_str(&arguments);
                    }
                }
            }
        }

        Ok(())
    }

    fn finish(self, provider: &str) -> Result<InferenceResponse, ProviderError> {
        let tool_calls = self
            .tool_calls
            .into_iter()
            .filter(|call| !call.name.is_empty())
            .map(|call| ToolCall {
                id: call.id,
                name: call.name,
                arguments: serde_json::from_str(&call.arguments)
                    .unwrap_or(Value::String(call.arguments)),
                provider_data: Default::default(),
            })
            .collect::<Vec<_>>();

        if !self.has_content && tool_calls.is_empty() && self.finish_reason.is_none() {
            return Err(ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: "stream produced no events".to_string(),
            });
        }

        Ok(InferenceResponse {
            content: self.has_content.then_some(self.content),
            tool_calls,
            finish_reason: self.finish_reason,
            reasoning: None,
            usage: self.usage,
            cost: None,
            provider_data: Default::default(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsStreamChunk {
    #[serde(default)]
    choices: Vec<ChatStreamChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    #[serde(default)]
    delta: ChatStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatStreamDelta {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatStreamToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamToolCall {
    index: Option<usize>,
    id: Option<String>,
    function: Option<ChatStreamToolCallFunction>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChatCompletionsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    messages: Vec<ChatCompletionsMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatCompletionsTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<ChatStreamOptions>,
}

#[derive(Debug, Serialize)]
struct ChatStreamOptions {
    include_usage: bool,
}

impl ChatCompletionsRequest {
    pub(crate) fn from_request(
        request: InferenceRequest,
        output_token_parameter: OutputTokenParameter,
    ) -> Self {
        let (max_tokens, max_completion_tokens) = match output_token_parameter {
            OutputTokenParameter::MaxTokens => (request.max_output_tokens, None),
            OutputTokenParameter::MaxCompletionTokens => (None, request.max_output_tokens),
        };
        Self {
            model: Some(request.model),
            messages: request
                .messages
                .into_iter()
                .map(ChatCompletionsMessage::from)
                .collect(),
            temperature: request.temperature,
            max_tokens,
            max_completion_tokens,
            tools: request
                .tools
                .into_iter()
                .map(ChatCompletionsTool::from)
                .collect(),
            stream: false,
            stream_options: None,
        }
    }

    pub(crate) fn without_model(mut self) -> Self {
        self.model = None;
        self
    }

    pub(crate) fn streaming(mut self) -> Self {
        self.stream = true;
        self.stream_options = Some(ChatStreamOptions {
            include_usage: true,
        });
        self
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionsMessage {
    role: &'static str,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ChatCompletionsToolCall>,
}

impl From<ChatMessage> for ChatCompletionsMessage {
    fn from(message: ChatMessage) -> Self {
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::Developer => "developer",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        Self {
            role,
            content: message.content,
            tool_call_id: message.tool_call_id,
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(ChatCompletionsToolCall::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionsToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatCompletionsToolCallFunction,
}

impl From<ToolCall> for ChatCompletionsToolCall {
    fn from(tool_call: ToolCall) -> Self {
        Self {
            id: tool_call.id.unwrap_or_else(|| tool_call.name.clone()),
            kind: "function",
            function: ChatCompletionsToolCallFunction {
                name: tool_call.name,
                arguments: serialize_tool_arguments(tool_call.arguments),
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionsToolCallFunction {
    name: String,
    arguments: String,
}

fn serialize_tool_arguments(arguments: Value) -> String {
    match arguments {
        Value::String(value) => value,
        other => other.to_string(),
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionsTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatCompletionsFunctionTool,
}

impl From<ToolDefinition> for ChatCompletionsTool {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            kind: "function",
            function: ChatCompletionsFunctionTool {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionsFunctionTool {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

impl ChatCompletionsResponse {
    pub(crate) fn normalize(self, provider: &str) -> Result<InferenceResponse, ProviderError> {
        let choice =
            self.choices
                .into_iter()
                .next()
                .ok_or_else(|| ProviderError::InvalidResponse {
                    provider: provider.to_string(),
                    message: "missing choices[0]".to_string(),
                })?;

        let ChatMessageResponse {
            content,
            tool_calls,
        } = choice.message;
        let tool_calls = tool_calls
            .into_iter()
            .map(ChatToolCallResponse::normalize)
            .collect();

        Ok(InferenceResponse {
            content,
            tool_calls,
            finish_reason: choice.finish_reason,
            reasoning: None,
            usage: self.usage.map(ChatUsage::normalize),
            cost: None,
            provider_data: Default::default(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    finish_reason: Option<String>,
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCallResponse>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCallResponse {
    id: Option<String>,
    function: ChatToolFunctionResponse,
}

impl ChatToolCallResponse {
    fn normalize(self) -> ToolCall {
        ToolCall {
            id: self.id,
            name: self.function.name,
            arguments: serde_json::from_str(&self.function.arguments)
                .unwrap_or(Value::String(self.function.arguments)),
            provider_data: Default::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatToolFunctionResponse {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    prompt_tokens_details: Option<PromptTokenDetails>,
    completion_tokens_details: Option<CompletionTokenDetails>,
}

impl ChatUsage {
    fn normalize(self) -> Usage {
        let prompt_total = self.prompt_tokens.unwrap_or_default();
        let cache_read = self
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .unwrap_or_default();
        let reasoning_tokens = self
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .unwrap_or_default();
        Usage {
            input_tokens: prompt_total.saturating_sub(cache_read),
            output_tokens: self.completion_tokens.unwrap_or_default(),
            cache_read_tokens: cache_read,
            cache_write_tokens: 0,
            reasoning_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PromptTokenDetails {
    cached_tokens: Option<u64>,
    #[allow(dead_code)]
    other: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CompletionTokenDetails {
    reasoning_tokens: Option<u64>,
    #[allow(dead_code)]
    other: Option<Value>,
}
