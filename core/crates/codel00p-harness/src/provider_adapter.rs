use async_trait::async_trait;
use codel00p_providers::{
    ChatMessage, InferenceClient, InferenceFallbackRoute, InferenceRequest, InferenceResponse,
    ToolDefinition,
};

use crate::{
    errors::HarnessError,
    memory::MemoryPromptAssembler,
    session::SessionMessage,
    skills::SkillPromptAssembler,
    turn::{
        HarnessInferenceRequest, HarnessInferenceResponse, ModelClient, ModelToolCall, TokenSink,
    },
};
use codel00p_protocol::SessionRole;

#[derive(Clone, Debug)]
pub struct ProviderModelClient {
    client: InferenceClient,
    provider: String,
    model: String,
    base_url: Option<String>,
    /// Provider/model candidates the inference client tries, in order, if the
    /// primary route fails with a fallback-eligible error (rate limit, overload,
    /// model-unavailable). Empty by default, so behavior is unchanged unless a
    /// caller wires routes in.
    fallback_routes: Vec<InferenceFallbackRoute>,
}

impl ProviderModelClient {
    pub fn new(
        client: InferenceClient,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            client,
            provider: provider.into(),
            model: model.into(),
            base_url: None,
            fallback_routes: Vec::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Attaches fallback routes that every inference request this client issues
    /// will carry. When the primary route fails with a fallback-eligible error,
    /// [`InferenceClient::complete`] retries against these routes in order.
    pub fn with_fallback_routes(mut self, routes: Vec<InferenceFallbackRoute>) -> Self {
        self.fallback_routes = routes;
        self
    }

    pub fn build_provider_request(
        provider: &str,
        model: &str,
        request: &HarnessInferenceRequest,
    ) -> InferenceRequest {
        let mut builder = InferenceRequest::builder(provider, model);

        if let Some(project_instructions) = request.project_instructions() {
            builder = builder.message(ChatMessage::system(project_instructions.as_prompt()));
        }

        if let Some(project_memory) = request.project_memory()
            && let Some(prompt) = MemoryPromptAssembler.assemble(project_memory)
        {
            builder = builder.message(ChatMessage::system(prompt));
        }

        if let Some(skills) = request.skills()
            && let Some(prompt) = SkillPromptAssembler.assemble(skills)
        {
            builder = builder.message(ChatMessage::system(prompt));
        }

        for message in request.session_state().messages() {
            builder = builder.message(map_session_message(message));
        }

        for tool in request.tools() {
            builder = builder.tool(ToolDefinition::function(
                &tool.name,
                &tool.description,
                tool.input_schema.clone(),
            ));
        }

        if let Some(tool_choice) = request.tool_choice() {
            builder = builder.tool_choice(tool_choice.clone());
        }

        if let Some(response_format) = request.response_format() {
            builder = builder.response_format(response_format.clone());
        }

        builder.build()
    }

    pub fn build_provider_request_with_base_url(
        provider: &str,
        model: &str,
        request: &HarnessInferenceRequest,
        base_url: Option<&str>,
    ) -> InferenceRequest {
        let mut request = Self::build_provider_request(provider, model, request);
        request.base_url = base_url.map(str::to_string);
        request
    }

    pub fn map_provider_response(
        provider: &str,
        model: &str,
        response: InferenceResponse,
    ) -> HarnessInferenceResponse {
        let tool_calls = response
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(index, tool_call)| {
                ModelToolCall::new(
                    tool_call
                        .id
                        .unwrap_or_else(|| format!("provider-tool-call-{index}")),
                    tool_call.name,
                    tool_call.arguments,
                )
            })
            .collect();

        let usage = response.usage.as_ref().map(map_usage);
        let cost = response.cost.as_ref().map(map_cost);

        HarnessInferenceResponse::from_parts(
            provider,
            model,
            response.content,
            tool_calls,
            response.finish_reason,
        )
        .with_usage(usage)
        .with_cost(cost)
    }
}

/// Translate a providers `Usage` record into the protocol-local mirror.
fn map_usage(usage: &codel00p_providers::Usage) -> codel00p_protocol::TokenUsage {
    codel00p_protocol::TokenUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        reasoning_tokens: usage.reasoning_tokens,
    }
}

/// Translate a providers cost estimate into the protocol-local mirror.
fn map_cost(cost: &codel00p_providers::UsageCostEstimate) -> codel00p_protocol::CostEstimate {
    codel00p_protocol::CostEstimate {
        currency: cost.currency.clone(),
        total_nanos: cost.total_nanos,
    }
}

#[async_trait]
impl ModelClient for ProviderModelClient {
    async fn infer(
        &self,
        request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        let mut provider_request = Self::build_provider_request_with_base_url(
            &self.provider,
            &self.model,
            &request,
            self.base_url.as_deref(),
        );
        provider_request
            .fallback_routes
            .clone_from(&self.fallback_routes);
        let response = self
            .client
            .complete(provider_request)
            .await
            .map_err(|error| HarnessError::InferenceFailed {
                message: error.to_string(),
            })?;

        Ok(Self::map_provider_response(
            &self.provider,
            &self.model,
            response,
        ))
    }

    async fn infer_streaming(
        &self,
        request: HarnessInferenceRequest,
        sink: &dyn TokenSink,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        let mut provider_request = Self::build_provider_request_with_base_url(
            &self.provider,
            &self.model,
            &request,
            self.base_url.as_deref(),
        );
        provider_request
            .fallback_routes
            .clone_from(&self.fallback_routes);
        let response = self
            .client
            .complete_streaming(provider_request, sink)
            .await
            .map_err(|error| HarnessError::InferenceFailed {
                message: error.to_string(),
            })?;

        Ok(Self::map_provider_response(
            &self.provider,
            &self.model,
            response,
        ))
    }
}

fn map_session_message(message: &SessionMessage) -> ChatMessage {
    match message.role() {
        SessionRole::System => ChatMessage::system(message.content().to_string()),
        SessionRole::User => ChatMessage::user(message.content().to_string()),
        SessionRole::Assistant if !message.tool_calls().is_empty() => {
            ChatMessage::assistant_tool_calls(
                message
                    .tool_calls()
                    .iter()
                    .map(|tool_call| codel00p_providers::ToolCall {
                        id: Some(tool_call.id().to_string()),
                        name: tool_call.name().to_string(),
                        arguments: tool_call.input().clone(),
                        provider_data: Default::default(),
                    })
                    .collect(),
            )
        }
        SessionRole::Assistant => ChatMessage::assistant(message.content().to_string()),
        SessionRole::Tool => ChatMessage::tool_result(
            message.tool_call_id().unwrap_or_default().to_string(),
            message.content().to_string(),
        ),
    }
}
