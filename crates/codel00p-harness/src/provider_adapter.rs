use async_trait::async_trait;
use codel00p_providers::{
    ChatMessage, InferenceClient, InferenceRequest, InferenceResponse, MessageRole, ToolDefinition,
};
use serde_json::json;

use crate::{
    errors::HarnessError,
    session::SessionMessage,
    turn::{HarnessInferenceRequest, HarnessInferenceResponse, ModelClient, ModelToolCall},
};

#[derive(Clone, Debug)]
pub struct ProviderModelClient {
    client: InferenceClient,
    provider: String,
    model: String,
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
        }
    }

    pub fn build_provider_request(
        provider: &str,
        model: &str,
        request: &HarnessInferenceRequest,
    ) -> InferenceRequest {
        let mut builder = InferenceRequest::builder(provider, model);

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
        let provider_request = Self::build_provider_request(&self.provider, &self.model, &request);
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
    match message {
        SessionMessage::User { content } => ChatMessage::user(content.clone()),
        SessionMessage::Assistant { content } => ChatMessage::assistant(content.clone()),
        SessionMessage::Tool { content, .. } => ChatMessage {
            role: MessageRole::Tool,
            content: content.clone(),
        },
    }
}
