use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole, ProviderError,
    ResponseFormat, TokenSink, ToolCall, ToolDefinition, Usage, transports::sse::stream_sse,
};

pub(crate) struct GeminiTransport {
    http: reqwest::Client,
}

impl GeminiTransport {
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

        let model = request.model.clone();
        let wire_request = GeminiRequest::from_request(provider, request)?;
        let response = self
            .http
            .post(generate_content_url(base_url, &model))
            .header("x-goog-api-key", api_key)
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

        let wire_response = response.json::<GeminiResponse>().await.map_err(|error| {
            ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            }
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

        let model = request.model.clone();
        let wire_request = GeminiRequest::from_request(provider, request)?;
        let response = self
            .http
            .post(stream_generate_content_url(base_url, &model))
            .header("x-goog-api-key", api_key)
            .json(&wire_request)
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        let mut accumulator = GeminiStreamAccumulator::default();
        stream_sse(provider, response, |event| {
            accumulator.ingest(provider, event, sink)?;
            Ok(false)
        })
        .await?;
        Ok(accumulator.finish())
    }
}

fn generate_content_url(base_url: &str, model: &str) -> String {
    let model = model.strip_prefix("models/").unwrap_or(model);
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1beta") {
        format!("{base_url}/models/{model}:generateContent")
    } else {
        format!("{base_url}/v1beta/models/{model}:generateContent")
    }
}

fn stream_generate_content_url(base_url: &str, model: &str) -> String {
    let model = model.strip_prefix("models/").unwrap_or(model);
    let base_url = base_url.trim_end_matches('/');
    if base_url.ends_with("/v1beta") {
        format!("{base_url}/models/{model}:streamGenerateContent?alt=sse")
    } else {
        format!("{base_url}/v1beta/models/{model}:streamGenerateContent?alt=sse")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<serde_json::Value>,
}

impl GeminiRequest {
    fn from_request(provider: &str, request: InferenceRequest) -> Result<Self, ProviderError> {
        let mut system_parts = Vec::new();
        let mut contents = Vec::new();
        let mut tool_call_names = BTreeMap::new();

        for message in request.messages {
            match message.role {
                MessageRole::System | MessageRole::Developer => {
                    if let Some(content) = message.content
                        && !content.is_empty()
                    {
                        system_parts.push(GeminiPart::Text { text: content });
                    }
                }
                MessageRole::Assistant => {
                    for tool_call in &message.tool_calls {
                        if let Some(id) = &tool_call.id {
                            tool_call_names.insert(id.clone(), tool_call.name.clone());
                        }
                    }
                    contents.push(GeminiContent::from_chat_message(
                        provider,
                        message,
                        &tool_call_names,
                    )?);
                }
                _ => contents.push(GeminiContent::from_chat_message(
                    provider,
                    message,
                    &tool_call_names,
                )?),
            }
        }

        let tool_config = (!request.tools.is_empty())
            .then(|| {
                request
                    .tool_choice
                    .as_ref()
                    .map(|choice| choice.gemini_config())
            })
            .flatten();
        Ok(Self {
            contents,
            system_instruction: GeminiSystemInstruction::from_parts(system_parts),
            generation_config: GeminiGenerationConfig::from_options(
                request.temperature,
                request.max_output_tokens,
                request.response_format.as_ref(),
            ),
            tools: GeminiTool::from_tools(request.tools),
            tool_config,
        })
    }
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

impl GeminiSystemInstruction {
    fn from_parts(parts: Vec<GeminiPart>) -> Option<Self> {
        if parts.is_empty() {
            None
        } else {
            Some(Self { parts })
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_schema: Option<Value>,
}

impl GeminiGenerationConfig {
    fn from_options(
        temperature: Option<f32>,
        max_output_tokens: Option<u32>,
        response_format: Option<&ResponseFormat>,
    ) -> Option<Self> {
        let response_mime_type = response_format
            .and_then(|format| format.gemini_mime_type())
            .map(str::to_string);
        let response_schema = response_format
            .and_then(|format| format.gemini_schema())
            .cloned();
        if temperature.is_none()
            && max_output_tokens.is_none()
            && response_mime_type.is_none()
            && response_schema.is_none()
        {
            None
        } else {
            Some(Self {
                temperature,
                max_output_tokens,
                response_mime_type,
                response_schema,
            })
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    role: &'static str,
    parts: Vec<GeminiPart>,
}

impl GeminiContent {
    fn from_chat_message(
        provider: &str,
        message: ChatMessage,
        tool_call_names: &BTreeMap<String, String>,
    ) -> Result<Self, ProviderError> {
        match message.role {
            MessageRole::User => Ok(Self {
                role: "user",
                parts: vec![GeminiPart::Text {
                    text: message.content.unwrap_or_default(),
                }],
            }),
            MessageRole::Assistant => Ok(Self {
                role: "model",
                parts: assistant_parts(message.content, message.tool_calls),
            }),
            MessageRole::Tool => Ok(Self {
                role: "user",
                parts: vec![GeminiPart::FunctionResponse {
                    function_response: GeminiFunctionResponse {
                        id: message.tool_call_id.clone(),
                        name: tool_result_name(provider, &message, tool_call_names)?,
                        response: parse_tool_response(message.content.unwrap_or_default()),
                    },
                }],
            }),
            MessageRole::System | MessageRole::Developer => unreachable!("handled by caller"),
        }
    }
}

fn assistant_parts(content: Option<String>, tool_calls: Vec<ToolCall>) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    if let Some(content) = content
        && !content.is_empty()
    {
        parts.push(GeminiPart::Text { text: content });
    }
    parts.extend(tool_calls.into_iter().map(GeminiPart::from));
    parts
}

fn tool_result_name(
    provider: &str,
    message: &ChatMessage,
    tool_call_names: &BTreeMap<String, String>,
) -> Result<String, ProviderError> {
    let tool_call_id =
        message
            .tool_call_id
            .clone()
            .ok_or_else(|| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: "tool result message is missing tool_call_id".to_string(),
            })?;
    Ok(tool_call_names
        .get(&tool_call_id)
        .cloned()
        .unwrap_or(tool_call_id))
}

fn parse_tool_response(content: String) -> Value {
    match serde_json::from_str::<Value>(&content) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => {
            let mut map = serde_json::Map::new();
            map.insert("result".to_string(), other);
            Value::Object(map)
        }
        Err(_) => {
            let mut map = serde_json::Map::new();
            map.insert("result".to_string(), Value::String(content));
            Value::Object(map)
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

impl From<ToolCall> for GeminiPart {
    fn from(tool_call: ToolCall) -> Self {
        Self::FunctionCall {
            function_call: GeminiFunctionCall {
                id: tool_call.id,
                name: tool_call.name,
                args: tool_call.arguments,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    name: String,
    args: Value,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    name: String,
    response: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

impl GeminiTool {
    fn from_tools(tools: Vec<ToolDefinition>) -> Vec<Self> {
        if tools.is_empty() {
            Vec::new()
        } else {
            vec![Self {
                function_declarations: tools
                    .into_iter()
                    .map(GeminiFunctionDeclaration::from)
                    .collect(),
            }]
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: Value,
}

impl From<ToolDefinition> for GeminiFunctionDeclaration {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            name: tool.name,
            description: tool.description,
            parameters: tool.parameters,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsageMetadata>,
    response_id: Option<String>,
    model_version: Option<String>,
}

impl GeminiResponse {
    fn normalize(self, provider: &str) -> Result<InferenceResponse, ProviderError> {
        let candidate =
            self.candidates
                .into_iter()
                .next()
                .ok_or_else(|| ProviderError::InvalidResponse {
                    provider: provider.to_string(),
                    message: "missing candidates[0]".to_string(),
                })?;

        let mut text_blocks = Vec::new();
        let mut tool_calls = Vec::new();
        for part in candidate.content.parts {
            match part {
                GeminiResponsePart::Text { text } => {
                    if !text.is_empty() {
                        text_blocks.push(text);
                    }
                }
                GeminiResponsePart::FunctionCall { function_call } => {
                    tool_calls.push(function_call.normalize());
                }
                GeminiResponsePart::Other {} => {}
            }
        }

        let mut provider_data = BTreeMap::new();
        if let Some(response_id) = self.response_id {
            provider_data.insert("response_id".to_string(), Value::String(response_id));
        }
        if let Some(model_version) = self.model_version {
            provider_data.insert("model_version".to_string(), Value::String(model_version));
        }

        Ok(InferenceResponse {
            content: if text_blocks.is_empty() {
                None
            } else {
                Some(text_blocks.join("\n"))
            },
            tool_calls,
            finish_reason: candidate.finish_reason,
            reasoning: None,
            usage: self.usage_metadata.map(GeminiUsageMetadata::normalize),
            cost: None,
            provider_data,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: GeminiResponseContent,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    #[allow(dead_code)]
    role: Option<String>,
    #[serde(default)]
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GeminiResponsePart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiResponseFunctionCall,
    },
    Other {},
}

#[derive(Debug, Deserialize)]
struct GeminiResponseFunctionCall {
    id: Option<String>,
    name: String,
    args: Option<Value>,
}

impl GeminiResponseFunctionCall {
    fn normalize(self) -> ToolCall {
        ToolCall {
            id: self.id,
            name: self.name,
            arguments: self.args.unwrap_or(Value::Object(Default::default())),
            provider_data: Default::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u64>,
    cached_content_token_count: Option<u64>,
    candidates_token_count: Option<u64>,
    thoughts_token_count: Option<u64>,
}

impl GeminiUsageMetadata {
    fn normalize(self) -> Usage {
        let input_total = self.prompt_token_count.unwrap_or_default();
        let cache_read = self.cached_content_token_count.unwrap_or_default();
        Usage {
            input_tokens: input_total.saturating_sub(cache_read),
            output_tokens: self.candidates_token_count.unwrap_or_default(),
            cache_read_tokens: cache_read,
            cache_write_tokens: 0,
            reasoning_tokens: self.thoughts_token_count.unwrap_or_default(),
        }
    }
}

#[derive(Default)]
struct GeminiStreamAccumulator {
    content: String,
    has_content: bool,
    tool_calls: Vec<ToolCall>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
}

impl GeminiStreamAccumulator {
    fn ingest(
        &mut self,
        provider: &str,
        payload: &str,
        sink: &dyn TokenSink,
    ) -> Result<(), ProviderError> {
        let chunk: GeminiResponse =
            serde_json::from_str(payload).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        if let Some(usage) = chunk.usage_metadata {
            self.usage = Some(usage.normalize());
        }

        if let Some(candidate) = chunk.candidates.into_iter().next() {
            if let Some(finish_reason) = candidate.finish_reason {
                self.finish_reason = Some(finish_reason);
            }
            for part in candidate.content.parts {
                match part {
                    GeminiResponsePart::Text { text } => {
                        if !text.is_empty() {
                            self.has_content = true;
                            self.content.push_str(&text);
                            sink.on_token(&text);
                        }
                    }
                    GeminiResponsePart::FunctionCall { function_call } => {
                        // Gemini delivers each function call atomically (the full
                        // argument object arrives in one part rather than as
                        // incremental fragments). Surface it as a single delta so
                        // callers still observe the call live; the assembled call
                        // pushed below is unchanged.
                        let call = function_call.normalize();
                        let index = self.tool_calls.len();
                        let args_fragment = match &call.arguments {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        sink.on_tool_call_delta(
                            index,
                            call.id.as_deref(),
                            Some(&call.name),
                            &args_fragment,
                        );
                        self.tool_calls.push(call);
                    }
                    GeminiResponsePart::Other {} => {}
                }
            }
        }

        Ok(())
    }

    fn finish(self) -> InferenceResponse {
        InferenceResponse {
            content: self.has_content.then_some(self.content),
            tool_calls: self.tool_calls,
            finish_reason: self.finish_reason,
            reasoning: None,
            usage: self.usage,
            cost: None,
            provider_data: Default::default(),
        }
    }
}
