use std::time::Duration;

use serde::Deserialize;

use crate::{Credential, ManagedIdentityCredentialRequest, ProviderError};

pub const AZURE_MANAGED_IDENTITY_TOKEN_ENDPOINT: &str =
    "http://169.254.169.254/metadata/identity/oauth2/token";

const AZURE_MANAGED_IDENTITY_API_VERSION: &str = "2018-02-01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AzureManagedIdentitySelector {
    SystemAssigned,
    ClientId(String),
    ObjectId(String),
    ResourceId(String),
}

#[derive(Debug, Clone)]
pub struct AzureManagedIdentityCredentialResolver {
    endpoint: String,
    resource: String,
    selector: AzureManagedIdentitySelector,
    http: reqwest::Client,
}

impl AzureManagedIdentityCredentialResolver {
    pub fn system_assigned(resource: impl Into<String>) -> Self {
        Self::new(resource, AzureManagedIdentitySelector::SystemAssigned)
    }

    pub fn user_assigned_client_id(
        resource: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self::new(
            resource,
            AzureManagedIdentitySelector::ClientId(client_id.into()),
        )
    }

    pub fn user_assigned_object_id(
        resource: impl Into<String>,
        object_id: impl Into<String>,
    ) -> Self {
        Self::new(
            resource,
            AzureManagedIdentitySelector::ObjectId(object_id.into()),
        )
    }

    pub fn user_assigned_resource_id(
        resource: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self::new(
            resource,
            AzureManagedIdentitySelector::ResourceId(resource_id.into()),
        )
    }

    pub fn new(resource: impl Into<String>, selector: AzureManagedIdentitySelector) -> Self {
        Self {
            endpoint: AZURE_MANAGED_IDENTITY_TOKEN_ENDPOINT.to_string(),
            resource: resource.into(),
            selector,
            http: azure_managed_identity_http_client(),
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
            .header("Metadata", "true")
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: format!("Azure IMDS token request failed: {error}"),
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("Azure IMDS token response could not be read: {error}"),
        })?;

        if !status.is_success() {
            return Err(ProviderError::Http {
                provider: provider.to_string(),
                message: format!(
                    "Azure IMDS token request for `{identity_ref}` failed with status {status}"
                ),
            });
        }

        let token: AzureManagedIdentityTokenResponse =
            serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: format!("Azure IMDS token response was not valid JSON: {error}"),
            })?;

        match token.access_token {
            Some(access_token) if !access_token.trim().is_empty() => {
                Ok(Credential::api_key(access_token))
            }
            _ => Err(ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: "Azure IMDS token response did not include access_token".to_string(),
            }),
        }
    }

    fn token_url(&self, provider: &str) -> Result<reqwest::Url, ProviderError> {
        let mut url = reqwest::Url::parse(&self.endpoint).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("Azure IMDS token endpoint is invalid: {error}"),
        })?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("api-version", AZURE_MANAGED_IDENTITY_API_VERSION);
            query.append_pair("resource", &self.resource);
            match &self.selector {
                AzureManagedIdentitySelector::SystemAssigned => {}
                AzureManagedIdentitySelector::ClientId(client_id) => {
                    query.append_pair("client_id", client_id);
                }
                AzureManagedIdentitySelector::ObjectId(object_id) => {
                    query.append_pair("object_id", object_id);
                }
                AzureManagedIdentitySelector::ResourceId(resource_id) => {
                    query.append_pair("msi_res_id", resource_id);
                }
            }
        }

        Ok(url)
    }
}

#[derive(Debug, Deserialize)]
struct AzureManagedIdentityTokenResponse {
    access_token: Option<String>,
}

fn azure_managed_identity_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build proxy-free Azure managed identity HTTP client")
}
