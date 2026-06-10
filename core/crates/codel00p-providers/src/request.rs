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

use serde_json::Value;

use crate::ToolCall;

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
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub base_url: Option<String>,
    pub fallback_routes: Vec<InferenceFallbackRoute>,
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
                temperature: None,
                max_output_tokens: None,
                base_url: None,
                fallback_routes: Vec::new(),
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

    pub fn build(self) -> InferenceRequest {
        self.request
    }
}
