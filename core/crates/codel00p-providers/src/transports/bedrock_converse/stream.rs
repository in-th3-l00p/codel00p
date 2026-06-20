//! Amazon eventstream decoding and streaming response accumulation.

use super::*;

pub(super) struct DecodedFrame {
    pub(super) event_type: Option<String>,
    pub(super) payload: Vec<u8>,
    pub(super) consumed: usize,
}

/// Decodes a single `vnd.amazon.eventstream` message from the front of
/// `buffer`, returning `None` until a whole message is buffered. Prelude and
/// message CRCs are not validated; the framing lengths are trusted.
pub(super) fn decode_eventstream_frame(
    buffer: &[u8],
) -> Result<Option<DecodedFrame>, ProviderError> {
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
pub(super) struct BedrockStreamAccumulator {
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
    pub(super) fn ingest(
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
                    let id = tool
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let name = tool
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    // Surface the opening of a tool call (id + name, no args yet)
                    // so callers can show it being assembled live. The final
                    // assembled call (built in `finish`) is unchanged.
                    sink.on_tool_call_delta(index, id.as_deref(), Some(&name), "");
                    self.tool_calls.insert(
                        index,
                        BedrockStreamToolCall {
                            id,
                            name,
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
                        let slot = self.tool_calls.entry(index).or_default();
                        slot.input.push_str(input);
                        sink.on_tool_call_delta(index, slot.id.as_deref(), None, input);
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

    pub(super) fn finish(self) -> InferenceResponse {
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
