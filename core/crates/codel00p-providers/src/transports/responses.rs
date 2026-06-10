use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole, ProviderError,
    ToolCall, ToolDefinition, Usage,
};

pub(crate) struct ResponsesTransport {
    http: reqwest::Client,
}

impl ResponsesTransport {
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

        let wire_request = ResponsesRequest::from_request(provider, request)?;
        let response = self
            .http
            .post(responses_url(base_url))
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
            .json::<ResponsesResponse>()
            .await
            .map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        wire_response.normalize(provider)
    }
}

fn responses_url(base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1") {
        format!("{base_url}/responses")
    } else {
        format!("{base_url}/v1/responses")
    }
}

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    model: String,
    input: Vec<ResponsesInputItem>,
    store: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ResponsesTool>,
}

impl ResponsesRequest {
    fn from_request(provider: &str, request: InferenceRequest) -> Result<Self, ProviderError> {
        let mut input = Vec::new();
        for message in request.messages {
            input.extend(ResponsesInputItem::from_chat_message(provider, message)?);
        }

        Ok(Self {
            model: request.model,
            input,
            store: false,
            temperature: request.temperature,
            max_output_tokens: request.max_output_tokens,
            tools: request.tools.into_iter().map(ResponsesTool::from).collect(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ResponsesInputItem {
    Message {
        role: &'static str,
        content: Vec<ResponsesInputContent>,
    },
    FunctionCall {
        #[serde(rename = "type")]
        kind: &'static str,
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        #[serde(rename = "type")]
        kind: &'static str,
        call_id: String,
        output: String,
    },
}

impl ResponsesInputItem {
    fn from_chat_message(provider: &str, message: ChatMessage) -> Result<Vec<Self>, ProviderError> {
        match message.role {
            MessageRole::System => Ok(vec![Self::text_message(
                "system",
                ResponsesInputTextType::InputText,
                message.content,
            )]),
            MessageRole::Developer => Ok(vec![Self::text_message(
                "developer",
                ResponsesInputTextType::InputText,
                message.content,
            )]),
            MessageRole::User => Ok(vec![Self::text_message(
                "user",
                ResponsesInputTextType::InputText,
                message.content,
            )]),
            MessageRole::Assistant => {
                let mut input = Vec::new();
                if let Some(content) = message.content
                    && !content.is_empty()
                {
                    input.push(Self::text_message(
                        "assistant",
                        ResponsesInputTextType::OutputText,
                        Some(content),
                    ));
                }
                input.extend(message.tool_calls.into_iter().map(Self::from_tool_call));
                Ok(input)
            }
            MessageRole::Tool => Ok(vec![Self::FunctionCallOutput {
                kind: "function_call_output",
                call_id: message
                    .tool_call_id
                    .ok_or_else(|| ProviderError::InvalidResponse {
                        provider: provider.to_string(),
                        message: "tool result message is missing tool_call_id".to_string(),
                    })?,
                output: message.content.unwrap_or_default(),
            }]),
        }
    }

    fn text_message(
        role: &'static str,
        text_type: ResponsesInputTextType,
        content: Option<String>,
    ) -> Self {
        Self::Message {
            role,
            content: vec![ResponsesInputContent {
                kind: text_type.as_str(),
                text: content.unwrap_or_default(),
            }],
        }
    }

    fn from_tool_call(tool_call: ToolCall) -> Self {
        Self::FunctionCall {
            kind: "function_call",
            call_id: tool_call.id.unwrap_or_else(|| tool_call.name.clone()),
            name: tool_call.name,
            arguments: serialize_tool_arguments(tool_call.arguments),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ResponsesInputTextType {
    InputText,
    OutputText,
}

impl ResponsesInputTextType {
    fn as_str(self) -> &'static str {
        match self {
            Self::InputText => "input_text",
            Self::OutputText => "output_text",
        }
    }
}

#[derive(Debug, Serialize)]
struct ResponsesInputContent {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

#[derive(Debug, Serialize)]
struct ResponsesTool {
    #[serde(rename = "type")]
    kind: &'static str,
    name: String,
    description: String,
    parameters: Value,
}

impl From<ToolDefinition> for ResponsesTool {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            kind: "function",
            name: tool.name,
            description: tool.description,
            parameters: tool.parameters,
        }
    }
}

fn serialize_tool_arguments(arguments: Value) -> String {
    match arguments {
        Value::String(value) => value,
        other => other.to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    id: Option<String>,
    status: Option<String>,
    model: Option<String>,
    #[serde(default)]
    output: Vec<ResponsesOutputItem>,
    usage: Option<ResponsesUsage>,
}

impl ResponsesResponse {
    fn normalize(self, provider: &str) -> Result<InferenceResponse, ProviderError> {
        let output_provider_data =
            serde_json::to_value(&self.output).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        let mut text_blocks = Vec::new();
        let mut tool_calls = Vec::new();

        for item in self.output {
            match item.kind.as_str() {
                "message" => text_blocks.extend(item.output_text_blocks()),
                "function_call" => tool_calls.push(item.normalize_function_call(provider)?),
                _ => {}
            }
        }

        let mut provider_data = BTreeMap::new();
        if let Some(id) = &self.id {
            provider_data.insert("id".to_string(), Value::String(id.clone()));
        }
        if let Some(model) = &self.model {
            provider_data.insert("model".to_string(), Value::String(model.clone()));
        }
        if let Some(status) = &self.status {
            provider_data.insert("status".to_string(), Value::String(status.clone()));
        }
        provider_data.insert("output".to_string(), output_provider_data);

        Ok(InferenceResponse {
            content: if text_blocks.is_empty() {
                None
            } else {
                Some(text_blocks.join("\n"))
            },
            tool_calls,
            finish_reason: self.status,
            reasoning: None,
            usage: self.usage.map(ResponsesUsage::normalize),
            cost: None,
            provider_data,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ResponsesOutputItem {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    status: Option<String>,
    role: Option<String>,
    #[serde(default)]
    content: Vec<ResponsesOutputContent>,
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl ResponsesOutputItem {
    fn output_text_blocks(self) -> Vec<String> {
        self.content
            .into_iter()
            .filter_map(ResponsesOutputContent::into_text)
            .collect()
    }

    fn normalize_function_call(self, provider: &str) -> Result<ToolCall, ProviderError> {
        let call_id = self.call_id.ok_or_else(|| ProviderError::InvalidResponse {
            provider: provider.to_string(),
            message: "function_call output is missing call_id".to_string(),
        })?;
        let name = self.name.ok_or_else(|| ProviderError::InvalidResponse {
            provider: provider.to_string(),
            message: "function_call output is missing name".to_string(),
        })?;
        let arguments = self.arguments.unwrap_or_default();

        let mut provider_data = BTreeMap::new();
        if let Some(id) = self.id {
            provider_data.insert("id".to_string(), Value::String(id));
        }
        if let Some(status) = self.status {
            provider_data.insert("status".to_string(), Value::String(status));
        }

        Ok(ToolCall {
            id: Some(call_id),
            name,
            arguments: serde_json::from_str(&arguments).unwrap_or(Value::String(arguments)),
            provider_data,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ResponsesOutputContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl ResponsesOutputContent {
    fn into_text(self) -> Option<String> {
        if self.kind == "output_text" {
            self.text.filter(|text| !text.is_empty())
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResponsesUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    input_tokens_details: Option<ResponsesInputTokenDetails>,
    output_tokens_details: Option<ResponsesOutputTokenDetails>,
}

impl ResponsesUsage {
    fn normalize(self) -> Usage {
        let input_total = self.input_tokens.unwrap_or_default();
        let cache_read = self
            .input_tokens_details
            .and_then(|details| details.cached_tokens)
            .unwrap_or_default();
        let reasoning_tokens = self
            .output_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .unwrap_or_default();

        Usage {
            input_tokens: input_total.saturating_sub(cache_read),
            output_tokens: self.output_tokens.unwrap_or_default(),
            cache_read_tokens: cache_read,
            cache_write_tokens: 0,
            reasoning_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResponsesInputTokenDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ResponsesOutputTokenDetails {
    reasoning_tokens: Option<u64>,
}
