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

mod request;
mod response;
mod signing;
mod stream;

use request::{BedrockConverseRequest, converse_stream_url, converse_url};
use response::BedrockConverseResponse;
use signing::sign_bedrock_request;
use stream::{BedrockStreamAccumulator, decode_eventstream_frame};

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
