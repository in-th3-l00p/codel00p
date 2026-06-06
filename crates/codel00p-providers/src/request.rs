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

/// Provider-neutral chat message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
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
            },
        }
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

    pub fn build(self) -> InferenceRequest {
        self.request
    }
}
use serde_json::Value;
