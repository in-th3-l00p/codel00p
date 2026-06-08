use async_trait::async_trait;
use codel00p_providers::{
    ChatMessage, InferenceClient, InferenceRequest, InferenceResponse, ToolDefinition,
};
use serde_json::json;

use crate::{
    errors::HarnessError,
    memory::MemoryPromptAssembler,
    session::SessionMessage,
    turn::{HarnessInferenceRequest, HarnessInferenceResponse, ModelClient, ModelToolCall},
};
use codel00p_protocol::SessionRole;

#[derive(Clone, Debug)]
pub struct ProviderModelClient {
    client: InferenceClient,
    provider: String,
    model: String,
    base_url: Option<String>,
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
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn build_provider_request(
        provider: &str,
        model: &str,
        request: &HarnessInferenceRequest,
    ) -> InferenceRequest {
        let mut builder = InferenceRequest::builder(provider, model);

        if let Some(project_memory) = request.project_memory()
            && let Some(prompt) = MemoryPromptAssembler.assemble(project_memory)
        {
            builder = builder.message(ChatMessage::system(prompt));
        }

        for message in request.session_state().messages() {
            builder = builder.message(map_session_message(message));
        }

        for tool_name in request.tool_names() {
            builder = builder.tool(ToolDefinition::function(
                tool_name,
                format!("codel00p harness tool: {tool_name}"),
                json!({ "type": "object" }),
            ));
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

        HarnessInferenceResponse::from_parts(
            provider,
            model,
            response.content,
            tool_calls,
            response.finish_reason,
        )
    }
}

#[async_trait]
impl ModelClient for ProviderModelClient {
    async fn infer(
        &self,
        request: HarnessInferenceRequest,
    ) -> Result<HarnessInferenceResponse, HarnessError> {
        let provider_request = Self::build_provider_request_with_base_url(
            &self.provider,
            &self.model,
            &request,
            self.base_url.as_deref(),
        );
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
