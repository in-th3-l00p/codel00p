use std::time::Duration;

use serde::Deserialize;

use crate::{Credential, ManagedIdentityCredentialRequest, ProviderError};

pub const GCP_MANAGED_IDENTITY_METADATA_ENDPOINT: &str = "http://metadata.google.internal";

#[derive(Debug, Clone)]
pub struct GcpManagedIdentityCredentialResolver {
    endpoint: String,
    service_account: String,
    http: reqwest::Client,
}

impl GcpManagedIdentityCredentialResolver {
    pub fn default_service_account() -> Self {
        Self::service_account("default")
    }

    pub fn service_account(service_account: impl Into<String>) -> Self {
        Self {
            endpoint: GCP_MANAGED_IDENTITY_METADATA_ENDPOINT.to_string(),
            service_account: service_account.into(),
            http: gcp_managed_identity_http_client(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    pub async fn resolve(
        &self,
        provider: impl Into<String>,
        identity_ref: impl Into<String>,
    ) -> Result<Credential, ProviderError> {
        let provider = provider.into();
        let identity_ref = identity_ref.into();
        self.resolve_request(ManagedIdentityCredentialRequest::new(
            &provider,
            &identity_ref,
        ))
        .await
    }

    pub async fn resolve_request(
        &self,
        request: ManagedIdentityCredentialRequest<'_>,
    ) -> Result<Credential, ProviderError> {
        let provider = request.provider();
        let identity_ref = request.identity_ref();
        let url = self.token_url(provider)?;
        let response = self
            .http
            .get(url)
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: format!("GCP metadata token request failed: {error}"),
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("GCP metadata token response could not be read: {error}"),
        })?;

        if !status.is_success() {
            return Err(ProviderError::Http {
                provider: provider.to_string(),
                message: format!(
                    "GCP metadata token request for `{identity_ref}` failed with status {status}"
                ),
            });
        }

        let token: GcpMetadataTokenResponse =
            serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: format!("GCP metadata token response was not valid JSON: {error}"),
            })?;

        token.into_credential(provider)
    }

    fn token_url(&self, provider: &str) -> Result<reqwest::Url, ProviderError> {
        let mut endpoint = self.endpoint.clone();
        if !endpoint.ends_with('/') {
            endpoint.push('/');
        }
        let base = reqwest::Url::parse(&endpoint).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("GCP metadata endpoint is invalid: {error}"),
        })?;
        let path = format!(
            "computeMetadata/v1/instance/service-accounts/{}/token",
            urlencoding::encode(&self.service_account)
        );
        base.join(&path).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("GCP metadata token path is invalid: {error}"),
        })
    }
}

#[derive(Debug, Deserialize)]
struct GcpMetadataTokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
}

impl GcpMetadataTokenResponse {
    fn into_credential(self, provider: &str) -> Result<Credential, ProviderError> {
        if let Some(token_type) = self.token_type
            && !token_type.eq_ignore_ascii_case("Bearer")
        {
            return Err(ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: format!("GCP metadata token response returned token_type `{token_type}`"),
            });
        }

        match self.access_token {
            Some(access_token) if !access_token.trim().is_empty() => {
                Ok(Credential::api_key(access_token))
            }
            _ => Err(ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: "GCP metadata token response did not include access_token".to_string(),
            }),
        }
    }
}

fn gcp_managed_identity_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build proxy-free GCP managed identity HTTP client")
}
