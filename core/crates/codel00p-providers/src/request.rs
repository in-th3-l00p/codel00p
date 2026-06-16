/// Provider-neutral chat message role.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

use serde_json::{Value, json};

use crate::{ToolCall, UsagePricing};

/// Provider-neutral chat message.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: None,
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

/// Provider-neutral inference request.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceRequest {
    pub provider: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: Option<ToolChoice>,
    pub response_format: Option<ResponseFormat>,
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub base_url: Option<String>,
    pub deployment: Option<String>,
    pub api_version: Option<String>,
    pub fallback_routes: Vec<InferenceFallbackRoute>,
    pub pricing: Option<UsagePricing>,
}

impl InferenceRequest {
    pub fn builder(
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> InferenceRequestBuilder {
        InferenceRequestBuilder {
            request: Self {
                provider: provider.into(),
                model: model.into(),
                messages: Vec::new(),
                tools: Vec::new(),
                tool_choice: None,
                response_format: None,
                temperature: None,
                max_output_tokens: None,
                base_url: None,
                deployment: None,
                api_version: None,
                fallback_routes: Vec::new(),
                pricing: None,
            },
        }
    }
}

/// Provider/model candidate used if the primary route can safely fall back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceFallbackRoute {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
}

impl InferenceFallbackRoute {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            base_url: None,
        }
    }

    pub fn base_url(mut self, value: impl Into<String>) -> Self {
        self.base_url = Some(value.into());
        self
    }
}

/// Provider-neutral function tool definition.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// Provider-neutral control over whether (and which) tool the model must call.
/// Maps to each provider's `tool_choice` / `tool_config` knob.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// The model decides whether to call a tool (the provider default).
    Auto,
    /// The model must call some tool.
    Required,
    /// The model must not call any tool.
    None,
    /// The model must call this specific tool.
    Tool(String),
}

impl ToolChoice {
    /// OpenAI-compatible chat-completions / Responses form (also Azure, GitHub).
    pub fn openai_value(&self) -> Value {
        match self {
            ToolChoice::Auto => json!("auto"),
            ToolChoice::Required => json!("required"),
            ToolChoice::None => json!("none"),
            ToolChoice::Tool(name) => json!({ "type": "function", "function": { "name": name } }),
        }
    }

    /// Anthropic Messages form.
    pub fn anthropic_value(&self) -> Value {
        match self {
            ToolChoice::Auto => json!({ "type": "auto" }),
            ToolChoice::Required => json!({ "type": "any" }),
            ToolChoice::None => json!({ "type": "none" }),
            ToolChoice::Tool(name) => json!({ "type": "tool", "name": name }),
        }
    }

    /// Gemini `toolConfig` form (`functionCallingConfig`). Keys are camelCase to
    /// match the Gemini REST API (a raw value is not subject to serde renaming).
    pub fn gemini_config(&self) -> Value {
        match self {
            ToolChoice::Auto => json!({ "functionCallingConfig": { "mode": "AUTO" } }),
            ToolChoice::Required => json!({ "functionCallingConfig": { "mode": "ANY" } }),
            ToolChoice::None => json!({ "functionCallingConfig": { "mode": "NONE" } }),
            ToolChoice::Tool(name) => json!({
                "functionCallingConfig": { "mode": "ANY", "allowedFunctionNames": [name] }
            }),
        }
    }

    /// AWS Bedrock Converse `toolChoice` form. `None` because Bedrock has no
    /// "none" choice — the caller omits tools entirely instead.
    pub fn bedrock_value(&self) -> Option<Value> {
        match self {
            ToolChoice::Auto => Some(json!({ "auto": {} })),
            ToolChoice::Required => Some(json!({ "any": {} })),
            ToolChoice::Tool(name) => Some(json!({ "tool": { "name": name } })),
            ToolChoice::None => Option::None,
        }
    }
}

/// Requests a structured (JSON) response, distinct from tool calling — for
/// planning output, structured edits, etc. Natively supported by OpenAI-style
/// chat completions (incl. Azure/GitHub) and Gemini; ignored by providers
/// without a native structured-output knob (Anthropic, Bedrock, the OpenAI
/// Responses API), where a forced single tool call is the idiomatic alternative.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Plain text (the default; serialized as nothing).
    Text,
    /// Any valid JSON object.
    JsonObject,
    /// A JSON object conforming to `schema` (provider-enforced where supported).
    JsonSchema { name: String, schema: Value },
}

impl ResponseFormat {
    /// OpenAI-compatible `response_format` value, or `None` for plain text.
    pub fn openai_value(&self) -> Option<Value> {
        match self {
            ResponseFormat::Text => None,
            ResponseFormat::JsonObject => Some(json!({ "type": "json_object" })),
            ResponseFormat::JsonSchema { name, schema } => Some(json!({
                "type": "json_schema",
                "json_schema": { "name": name, "strict": true, "schema": schema }
            })),
        }
    }

    /// Gemini `responseMimeType`, or `None` for plain text.
    pub fn gemini_mime_type(&self) -> Option<&'static str> {
        match self {
            ResponseFormat::Text => None,
            ResponseFormat::JsonObject | ResponseFormat::JsonSchema { .. } => {
                Some("application/json")
            }
        }
    }

    /// Gemini `responseSchema`, present only for [`ResponseFormat::JsonSchema`].
    pub fn gemini_schema(&self) -> Option<&Value> {
        match self {
            ResponseFormat::JsonSchema { schema, .. } => Some(schema),
            _ => None,
        }
    }
}

/// Builder for [`InferenceRequest`].
#[derive(Debug, Clone)]
pub struct InferenceRequestBuilder {
    request: InferenceRequest,
}

impl InferenceRequestBuilder {
    pub fn message(mut self, message: ChatMessage) -> Self {
        self.request.messages.push(message);
        self
    }

    pub fn tool(mut self, tool: ToolDefinition) -> Self {
        self.request.tools.push(tool);
        self
    }

    /// Controls whether/which tool the model must call. Without this the provider
    /// default (auto) applies.
    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.request.tool_choice = Some(choice);
        self
    }

    /// Requests a structured (JSON) response where the provider supports it.
    pub fn response_format(mut self, format: ResponseFormat) -> Self {
        self.request.response_format = Some(format);
        self
    }

    pub fn temperature(mut self, value: f32) -> Self {
        self.request.temperature = Some(value);
        self
    }

    pub fn max_output_tokens(mut self, value: u32) -> Self {
        self.request.max_output_tokens = Some(value);
        self
    }

    pub fn base_url(mut self, value: impl Into<String>) -> Self {
        self.request.base_url = Some(value.into());
        self
    }

    pub fn deployment(mut self, value: impl Into<String>) -> Self {
        self.request.deployment = Some(value.into());
        self
    }

    pub fn api_version(mut self, value: impl Into<String>) -> Self {
        self.request.api_version = Some(value.into());
        self
    }

    pub fn fallback_route(mut self, provider: impl Into<String>, model: impl Into<String>) -> Self {
        self.request
            .fallback_routes
            .push(InferenceFallbackRoute::new(provider, model));
        self
    }

    pub fn fallback_route_with_base_url(
        mut self,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        self.request
            .fallback_routes
            .push(InferenceFallbackRoute::new(provider, model).base_url(base_url));
        self
    }

    pub fn pricing(mut self, value: UsagePricing) -> Self {
        self.request.pricing = Some(value);
        self
    }

    pub fn build(self) -> InferenceRequest {
        self.request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_tool_choice_forms() {
        assert_eq!(ToolChoice::Auto.openai_value(), json!("auto"));
        assert_eq!(ToolChoice::Required.openai_value(), json!("required"));
        assert_eq!(ToolChoice::None.openai_value(), json!("none"));
        assert_eq!(
            ToolChoice::Tool("read_file".into()).openai_value(),
            json!({ "type": "function", "function": { "name": "read_file" } })
        );
    }

    #[test]
    fn anthropic_tool_choice_forms() {
        assert_eq!(
            ToolChoice::Auto.anthropic_value(),
            json!({ "type": "auto" })
        );
        assert_eq!(
            ToolChoice::Required.anthropic_value(),
            json!({ "type": "any" })
        );
        assert_eq!(
            ToolChoice::None.anthropic_value(),
            json!({ "type": "none" })
        );
        assert_eq!(
            ToolChoice::Tool("read_file".into()).anthropic_value(),
            json!({ "type": "tool", "name": "read_file" })
        );
    }

    #[test]
    fn gemini_tool_choice_forms_are_camel_case() {
        assert_eq!(
            ToolChoice::Required.gemini_config(),
            json!({ "functionCallingConfig": { "mode": "ANY" } })
        );
        assert_eq!(
            ToolChoice::Tool("read_file".into()).gemini_config(),
            json!({
                "functionCallingConfig": { "mode": "ANY", "allowedFunctionNames": ["read_file"] }
            })
        );
    }

    #[test]
    fn bedrock_tool_choice_forms_omit_none() {
        assert_eq!(
            ToolChoice::Auto.bedrock_value(),
            Some(json!({ "auto": {} }))
        );
        assert_eq!(
            ToolChoice::Required.bedrock_value(),
            Some(json!({ "any": {} }))
        );
        assert_eq!(
            ToolChoice::Tool("read_file".into()).bedrock_value(),
            Some(json!({ "tool": { "name": "read_file" } }))
        );
        // Bedrock has no "none"; the transport omits tools instead.
        assert_eq!(ToolChoice::None.bedrock_value(), None);
    }

    #[test]
    fn response_format_openai_values() {
        assert_eq!(ResponseFormat::Text.openai_value(), None);
        assert_eq!(
            ResponseFormat::JsonObject.openai_value(),
            Some(json!({ "type": "json_object" }))
        );
        let schema = json!({ "type": "object", "properties": { "ok": { "type": "boolean" } } });
        assert_eq!(
            ResponseFormat::JsonSchema {
                name: "result".into(),
                schema: schema.clone()
            }
            .openai_value(),
            Some(json!({
                "type": "json_schema",
                "json_schema": { "name": "result", "strict": true, "schema": schema }
            }))
        );
    }

    #[test]
    fn response_format_gemini_values() {
        assert_eq!(ResponseFormat::Text.gemini_mime_type(), None);
        assert_eq!(ResponseFormat::Text.gemini_schema(), None);
        assert_eq!(
            ResponseFormat::JsonObject.gemini_mime_type(),
            Some("application/json")
        );
        assert_eq!(ResponseFormat::JsonObject.gemini_schema(), None);
        let schema = json!({ "type": "object" });
        let format = ResponseFormat::JsonSchema {
            name: "r".into(),
            schema: schema.clone(),
        };
        assert_eq!(format.gemini_mime_type(), Some("application/json"));
        assert_eq!(format.gemini_schema(), Some(&schema));
    }
}
