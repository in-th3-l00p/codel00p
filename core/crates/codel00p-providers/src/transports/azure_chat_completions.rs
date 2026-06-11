use crate::{
    Credential, InferenceRequest, InferenceResponse, OutputTokenParameter, ProviderError,
    TokenSink,
    transports::chat_completions::{
        ChatCompletionsRequest, ChatCompletionsResponse, stream_chat_completions_response,
    },
};

const DEFAULT_AZURE_API_VERSION: &str = "2024-10-21";

pub(crate) struct AzureChatCompletionsTransport {
    http: reqwest::Client,
}

impl AzureChatCompletionsTransport {
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
        output_token_parameter: OutputTokenParameter,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, ProviderError> {
        let Credential::ApiKey(api_key) = credential else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let deployment = request
            .deployment
            .clone()
            .unwrap_or_else(|| request.model.clone());
        let api_version = request
            .api_version
            .clone()
            .unwrap_or_else(|| DEFAULT_AZURE_API_VERSION.to_string());
        let wire_request =
            ChatCompletionsRequest::from_request(request, output_token_parameter).without_model();
        let url = azure_chat_completions_url(base_url, &deployment, &api_version);

        let response = self
            .http
            .post(url)
            .header("api-key", api_key)
            .json(&wire_request)
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
            .json::<ChatCompletionsResponse>()
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
        output_token_parameter: OutputTokenParameter,
        request: InferenceRequest,
        sink: &dyn TokenSink,
    ) -> Result<InferenceResponse, ProviderError> {
        let Credential::ApiKey(api_key) = credential else {
            return Err(ProviderError::MissingCredential {
                provider: provider.to_string(),
            });
        };

        let deployment = request
            .deployment
            .clone()
            .unwrap_or_else(|| request.model.clone());
        let api_version = request
            .api_version
            .clone()
            .unwrap_or_else(|| DEFAULT_AZURE_API_VERSION.to_string());
        let wire_request = ChatCompletionsRequest::from_request(request, output_token_parameter)
            .without_model()
            .streaming();
        let url = azure_chat_completions_url(base_url, &deployment, &api_version);

        let response = self
            .http
            .post(url)
            .header("api-key", api_key)
            .json(&wire_request)
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: error.to_string(),
            })?;

        stream_chat_completions_response(provider, response, sink).await
    }
}

fn azure_chat_completions_url(base_url: &str, deployment: &str, api_version: &str) -> String {
    format!(
        "{}/openai/deployments/{}/chat/completions?api-version={}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(deployment),
        urlencoding::encode(api_version)
    )
}
