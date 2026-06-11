/// Wire protocol used for a provider request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiMode {
    ChatCompletions,
    AzureChatCompletions,
    AnthropicMessages,
    Responses,
    BedrockConverse,
    Gemini,
    ExternalProcess,
}

/// How credentials are resolved for a provider.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum AuthType {
    ApiKey,
    OAuthExternal,
    AwsSdk,
    GitHubCopilot,
    CloudProxy,
    Custom,
}

/// Request field used for output token limits in OpenAI-compatible protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputTokenParameter {
    MaxTokens,
    MaxCompletionTokens,
}

/// High-level provider and model capabilities used by policy and UI layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderCapabilities {
    pub tools: bool,
    pub streaming: bool,
    pub vision: bool,
    pub reasoning: bool,
}

impl ProviderCapabilities {
    pub const fn agentic() -> Self {
        Self {
            tools: true,
            streaming: true,
            vision: false,
            reasoning: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.tools && !self.streaming && !self.vision && !self.reasoning
    }
}

/// Declarative provider metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfile {
    pub id: &'static str,
    pub aliases: &'static [&'static str],
    pub display_name: &'static str,
    pub description: &'static str,
    pub api_mode: ApiMode,
    pub auth_type: AuthType,
    pub env_vars: &'static [&'static str],
    pub default_base_url: Option<&'static str>,
    pub models_url: Option<&'static str>,
    pub default_aux_model: Option<&'static str>,
    pub output_token_parameter: OutputTokenParameter,
    pub capabilities: ProviderCapabilities,
}
