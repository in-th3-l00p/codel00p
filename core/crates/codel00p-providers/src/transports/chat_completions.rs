use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole,
    OutputTokenParameter, ProviderError, ToolCall, ToolDefinition, Usage,
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
}

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<ChatCompletionsMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatCompletionsTool>,
}

impl ChatCompletionsRequest {
    fn from_request(
        request: InferenceRequest,
        output_token_parameter: OutputTokenParameter,
    ) -> Self {
        let (max_tokens, max_completion_tokens) = match output_token_parameter {
            OutputTokenParameter::MaxTokens => (request.max_output_tokens, None),
            OutputTokenParameter::MaxCompletionTokens => (None, request.max_output_tokens),
        };
        Self {
            model: request.model,
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
        }
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
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

impl ChatCompletionsResponse {
    fn normalize(self, provider: &str) -> Result<InferenceResponse, ProviderError> {
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
}

impl ChatUsage {
    fn normalize(self) -> Usage {
        let prompt_total = self.prompt_tokens.unwrap_or_default();
        let cache_read = self
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .unwrap_or_default();
        Usage {
            input_tokens: prompt_total.saturating_sub(cache_read),
            output_tokens: self.completion_tokens.unwrap_or_default(),
            cache_read_tokens: cache_read,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PromptTokenDetails {
    cached_tokens: Option<u64>,
    #[allow(dead_code)]
    other: Option<Value>,
}
