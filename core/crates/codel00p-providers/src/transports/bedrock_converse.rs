use std::collections::BTreeMap;

use futures::StreamExt;
use hmac::{Hmac, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::{
    ChatMessage, Credential, InferenceRequest, InferenceResponse, MessageRole, ProviderError,
    TokenSink, ToolCall, ToolDefinition, Usage,
};

type HmacSha256 = Hmac<Sha256>;

const AWS_ALGORITHM: &str = "AWS4-HMAC-SHA256";
const BEDROCK_SERVICE: &str = "bedrock";

pub(crate) struct BedrockConverseTransport {
    http: reqwest::Client,
}

impl BedrockConverseTransport {
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
        let Credential::AwsSigV4 {
            access_key_id,
            secret_access_key,
            session_token,
            region,
        } = credential
        else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let model = request.model.clone();
        let wire_request = BedrockConverseRequest::from_request(provider, request)?;
        let body =
            serde_json::to_vec(&wire_request).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;
        let url = converse_url(base_url, region, &model);
        let url = reqwest::Url::parse(&url).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: error.to_string(),
        })?;
        let headers = sign_bedrock_request(
            &url,
            &body,
            access_key_id,
            secret_access_key,
            session_token.as_deref(),
            region,
        )?;

        let response = self
            .http
            .post(url)
            .headers(headers)
            .body(body)
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
            .json::<BedrockConverseResponse>()
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
        let Credential::AwsSigV4 {
            access_key_id,
            secret_access_key,
            session_token,
            region,
        } = credential
        else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let model = request.model.clone();
        let wire_request = BedrockConverseRequest::from_request(provider, request)?;
        let body =
            serde_json::to_vec(&wire_request).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;
        let url = converse_stream_url(base_url, region, &model);
        let url = reqwest::Url::parse(&url).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: error.to_string(),
        })?;
        let headers = sign_bedrock_request(
            &url,
            &body,
            access_key_id,
            secret_access_key,
            session_token.as_deref(),
            region,
        )?;

        let response = self
            .http
            .post(url)
            .headers(headers)
            .body(body)
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

        let mut accumulator = BedrockStreamAccumulator::default();
        let mut buffer: Vec<u8> = Vec::new();
        let mut body = response.bytes_stream();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;
            buffer.extend_from_slice(&chunk);

            while let Some(frame) = decode_eventstream_frame(&buffer)? {
                if let Some(event_type) = frame.event_type {
                    accumulator.ingest(provider, &event_type, &frame.payload, sink)?;
                }
                buffer.drain(..frame.consumed);
            }
        }

        Ok(accumulator.finish())
    }
}

fn converse_url(base_url: &str, region: &str, model: &str) -> String {
    let model = urlencoding::encode(model);
    let base_url = bedrock_base_url(base_url, region);
    format!("{base_url}/model/{model}/converse")
}

fn converse_stream_url(base_url: &str, region: &str, model: &str) -> String {
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
struct BedrockConverseRequest {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<BedrockSystemBlock>,
    messages: Vec<BedrockMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inference_config: Option<BedrockInferenceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<BedrockToolConfig>,
}

impl BedrockConverseRequest {
    fn from_request(provider: &str, request: InferenceRequest) -> Result<Self, ProviderError> {
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
            tool_config: BedrockToolConfig::from_tools(request.tools),
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
}

impl BedrockToolConfig {
    fn from_tools(tools: Vec<ToolDefinition>) -> Option<Self> {
        if tools.is_empty() {
            None
        } else {
            Some(Self {
                tools: tools.into_iter().map(BedrockTool::from).collect(),
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BedrockConverseResponse {
    output: Option<BedrockOutput>,
    stop_reason: Option<String>,
    usage: Option<BedrockUsage>,
}

impl BedrockConverseResponse {
    fn normalize(self, _provider: &str) -> Result<InferenceResponse, ProviderError> {
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

fn sign_bedrock_request(
    url: &reqwest::Url,
    body: &[u8],
    access_key_id: &str,
    secret_access_key: &str,
    session_token: Option<&str>,
    region: &str,
) -> Result<HeaderMap, ProviderError> {
    let now = OffsetDateTime::now_utc();
    let date_stamp = format!(
        "{:04}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    );
    let amz_date = format!(
        "{date_stamp}T{:02}{:02}{:02}Z",
        now.hour(),
        now.minute(),
        now.second()
    );
    let payload_hash = sha256_hex(body);
    let host = canonical_host(url);

    let mut signed_headers = vec![
        ("content-type", "application/json".to_string()),
        ("host", host.clone()),
        ("x-amz-content-sha256", payload_hash.clone()),
        ("x-amz-date", amz_date.clone()),
    ];
    if let Some(session_token) = session_token {
        signed_headers.push(("x-amz-security-token", session_token.to_string()));
    }
    signed_headers.sort_by_key(|(name, _)| *name);

    let canonical_headers = signed_headers
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect::<String>();
    let signed_header_names = signed_headers
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "POST\n{}\n\n{}{}\n{}",
        url.path(),
        canonical_headers,
        signed_header_names,
        payload_hash
    );
    let credential_scope = format!("{date_stamp}/{region}/{BEDROCK_SERVICE}/aws4_request");
    let string_to_sign = format!(
        "{AWS_ALGORITHM}\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = aws_signing_key(secret_access_key, &date_stamp, region, BEDROCK_SERVICE);
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "{AWS_ALGORITHM} Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_header_names}, Signature={signature}"
    );

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        HOST,
        HeaderValue::from_str(&host).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );
    headers.insert(
        HeaderName::from_static("x-amz-content-sha256"),
        HeaderValue::from_str(&payload_hash).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );
    headers.insert(
        HeaderName::from_static("x-amz-date"),
        HeaderValue::from_str(&amz_date).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );
    if let Some(session_token) = session_token {
        headers.insert(
            HeaderName::from_static("x-amz-security-token"),
            HeaderValue::from_str(session_token).map_err(|error| ProviderError::Http {
                provider: "bedrock".to_string(),
                message: error.to_string(),
            })?,
        );
    }
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&authorization).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );

    Ok(headers)
}

fn canonical_host(url: &reqwest::Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn aws_signing_key(
    secret_access_key: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
) -> Vec<u8> {
    let date_key = hmac_sha256(
        format!("AWS4{secret_access_key}").as_bytes(),
        date_stamp.as_bytes(),
    );
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, service.as_bytes());
    hmac_sha256(&service_key, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

struct DecodedFrame {
    event_type: Option<String>,
    payload: Vec<u8>,
    consumed: usize,
}

/// Decodes a single `vnd.amazon.eventstream` message from the front of
/// `buffer`, returning `None` until a whole message is buffered. Prelude and
/// message CRCs are not validated; the framing lengths are trusted.
fn decode_eventstream_frame(buffer: &[u8]) -> Result<Option<DecodedFrame>, ProviderError> {
    if buffer.len() < 12 {
        return Ok(None);
    }

    let total_len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
    let headers_len = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
    if total_len < 16 || headers_len > total_len - 16 {
        return Err(ProviderError::InvalidResponse {
            provider: BEDROCK_SERVICE.to_string(),
            message: "invalid eventstream frame lengths".to_string(),
        });
    }
    if buffer.len() < total_len {
        return Ok(None);
    }

    let headers = &buffer[12..12 + headers_len];
    let payload = &buffer[12 + headers_len..total_len - 4];
    Ok(Some(DecodedFrame {
        event_type: event_type_from_headers(headers),
        payload: payload.to_vec(),
        consumed: total_len,
    }))
}

/// Extracts the `:event-type` string header from an eventstream header block,
/// stepping over any other typed headers.
fn event_type_from_headers(mut headers: &[u8]) -> Option<String> {
    let mut event_type = None;
    while !headers.is_empty() {
        let name_len = headers[0] as usize;
        if headers.len() < 1 + name_len + 1 {
            break;
        }
        let name = &headers[1..1 + name_len];
        let value_type = headers[1 + name_len];
        let mut cursor = 1 + name_len + 1;
        let value_len = match value_type {
            0 | 1 => 0,
            2 => 1,
            3 => 2,
            4 => 4,
            5 => 8,
            6 | 7 => {
                if headers.len() < cursor + 2 {
                    break;
                }
                let len = u16::from_be_bytes([headers[cursor], headers[cursor + 1]]) as usize;
                cursor += 2;
                len
            }
            8 => 8,
            9 => 16,
            _ => break,
        };
        if headers.len() < cursor + value_len {
            break;
        }
        if name == b":event-type" && value_type == 7 {
            event_type =
                Some(String::from_utf8_lossy(&headers[cursor..cursor + value_len]).to_string());
        }
        headers = &headers[cursor + value_len..];
    }
    event_type
}

#[derive(Default)]
struct BedrockStreamAccumulator {
    content: String,
    has_content: bool,
    tool_calls: BTreeMap<usize, BedrockStreamToolCall>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
}

#[derive(Default)]
struct BedrockStreamToolCall {
    id: Option<String>,
    name: String,
    input: String,
}

impl BedrockStreamAccumulator {
    fn ingest(
        &mut self,
        provider: &str,
        event_type: &str,
        payload: &[u8],
        sink: &dyn TokenSink,
    ) -> Result<(), ProviderError> {
        let value: Value =
            serde_json::from_slice(payload).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        match event_type {
            "contentBlockStart" => {
                let index = content_block_index(&value);
                if let Some(tool) = value.get("start").and_then(|start| start.get("toolUse")) {
                    self.tool_calls.insert(
                        index,
                        BedrockStreamToolCall {
                            id: tool
                                .get("toolUseId")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            name: tool
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            input: String::new(),
                        },
                    );
                }
            }
            "contentBlockDelta" => {
                let index = content_block_index(&value);
                if let Some(delta) = value.get("delta") {
                    if let Some(text) = delta.get("text").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        self.has_content = true;
                        self.content.push_str(text);
                        sink.on_token(text);
                    }
                    if let Some(input) = delta
                        .get("toolUse")
                        .and_then(|tool| tool.get("input"))
                        .and_then(Value::as_str)
                    {
                        self.tool_calls
                            .entry(index)
                            .or_default()
                            .input
                            .push_str(input);
                    }
                }
            }
            "messageStop" => {
                if let Some(reason) = value.get("stopReason").and_then(Value::as_str) {
                    self.finish_reason = Some(reason.to_string());
                }
            }
            "metadata" => {
                if let Some(usage) = value.get("usage") {
                    self.usage = Some(Usage {
                        input_tokens: usage_field(usage, "inputTokens"),
                        output_tokens: usage_field(usage, "outputTokens"),
                        cache_read_tokens: usage_field(usage, "cacheReadInputTokens"),
                        cache_write_tokens: usage_field(usage, "cacheWriteInputTokens"),
                        reasoning_tokens: 0,
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn finish(self) -> InferenceResponse {
        let tool_calls = self
            .tool_calls
            .into_values()
            .filter(|call| !call.name.is_empty())
            .map(|call| ToolCall {
                id: call.id,
                name: call.name,
                arguments: if call.input.is_empty() {
                    Value::Object(Default::default())
                } else {
                    serde_json::from_str(&call.input).unwrap_or(Value::String(call.input))
                },
                provider_data: Default::default(),
            })
            .collect();

        InferenceResponse {
            content: self.has_content.then_some(self.content),
            tool_calls,
            finish_reason: self.finish_reason,
            reasoning: None,
            usage: self.usage,
            cost: None,
            provider_data: Default::default(),
        }
    }
}

fn content_block_index(value: &Value) -> usize {
    value
        .get("contentBlockIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
}

fn usage_field(usage: &Value, field: &str) -> u64 {
    usage.get(field).and_then(Value::as_u64).unwrap_or_default()
}
