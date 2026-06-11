use std::time::Duration;

use serde::Deserialize;

use crate::{Credential, ManagedIdentityCredentialRequest, ProviderError};

pub const AWS_MANAGED_IDENTITY_IMDS_ENDPOINT: &str = "http://169.254.169.254";

const AWS_IMDS_TOKEN_TTL_SECONDS: u32 = 21_600;

#[derive(Debug, Clone)]
pub struct AwsManagedIdentityCredentialResolver {
    endpoint: String,
    region: String,
    role_name: Option<String>,
    token_ttl_seconds: u32,
    http: reqwest::Client,
}

impl AwsManagedIdentityCredentialResolver {
    pub fn instance_profile(region: impl Into<String>) -> Self {
        Self {
            endpoint: AWS_MANAGED_IDENTITY_IMDS_ENDPOINT.to_string(),
            region: region.into(),
            role_name: None,
            token_ttl_seconds: AWS_IMDS_TOKEN_TTL_SECONDS,
            http: aws_managed_identity_http_client(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_role_name(mut self, role_name: impl Into<String>) -> Self {
        self.role_name = Some(role_name.into());
        self
    }

    pub fn with_token_ttl_seconds(mut self, token_ttl_seconds: u32) -> Self {
        self.token_ttl_seconds = token_ttl_seconds;
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
        let token = self.fetch_imds_token(request).await?;
        let role_name = self.resolve_role_name(request, &token).await?;
        let credentials = self
            .fetch_role_credentials(request, &token, &role_name)
            .await?;
        credentials.into_credential(request.provider(), &self.region)
    }

    async fn fetch_imds_token(
        &self,
        request: ManagedIdentityCredentialRequest<'_>,
    ) -> Result<String, ProviderError> {
        let provider = request.provider();
        let identity_ref = request.identity_ref();
        let url = self.endpoint_url("latest/api/token", provider)?;
        let response = self
            .http
            .put(url)
            .header(
                "X-aws-ec2-metadata-token-ttl-seconds",
                self.token_ttl_seconds.to_string(),
            )
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: format!("AWS IMDSv2 token request failed: {error}"),
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("AWS IMDSv2 token response could not be read: {error}"),
        })?;

        if !status.is_success() {
            return Err(ProviderError::Http {
                provider: provider.to_string(),
                message: format!(
                    "AWS IMDSv2 token request for `{identity_ref}` failed with status {status}"
                ),
            });
        }

        let token = body.trim();
        if token.is_empty() {
            return Err(ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: "AWS IMDSv2 token response was empty".to_string(),
            });
        }

        Ok(token.to_string())
    }

    async fn resolve_role_name(
        &self,
        request: ManagedIdentityCredentialRequest<'_>,
        token: &str,
    ) -> Result<String, ProviderError> {
        if let Some(role_name) = &self.role_name {
            return Ok(role_name.clone());
        }

        let body = self
            .get_metadata_text("latest/meta-data/iam/security-credentials/", request, token)
            .await?;
        body.lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ProviderError::InvalidResponse {
                provider: request.provider().to_string(),
                message: "AWS IMDS role list did not include a role name".to_string(),
            })
    }

    async fn fetch_role_credentials(
        &self,
        request: ManagedIdentityCredentialRequest<'_>,
        token: &str,
        role_name: &str,
    ) -> Result<AwsImdsRoleCredentials, ProviderError> {
        let path = format!(
            "latest/meta-data/iam/security-credentials/{}",
            urlencoding::encode(role_name)
        );
        let body = self.get_metadata_text(&path, request, token).await?;
        serde_json::from_str(&body).map_err(|error| ProviderError::InvalidResponse {
            provider: request.provider().to_string(),
            message: format!("AWS IMDS role credential response was not valid JSON: {error}"),
        })
    }

    async fn get_metadata_text(
        &self,
        path: &str,
        request: ManagedIdentityCredentialRequest<'_>,
        token: &str,
    ) -> Result<String, ProviderError> {
        let provider = request.provider();
        let identity_ref = request.identity_ref();
        let url = self.endpoint_url(path, provider)?;
        let response = self
            .http
            .get(url)
            .header("X-aws-ec2-metadata-token", token)
            .send()
            .await
            .map_err(|error| ProviderError::Http {
                provider: provider.to_string(),
                message: format!("AWS IMDS metadata request failed: {error}"),
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("AWS IMDS metadata response could not be read: {error}"),
        })?;

        if !status.is_success() {
            return Err(ProviderError::Http {
                provider: provider.to_string(),
                message: format!(
                    "AWS IMDS metadata request for `{identity_ref}` failed with status {status}"
                ),
            });
        }

        Ok(body)
    }

    fn endpoint_url(&self, path: &str, provider: &str) -> Result<reqwest::Url, ProviderError> {
        let mut endpoint = self.endpoint.clone();
        if !endpoint.ends_with('/') {
            endpoint.push('/');
        }
        let base = reqwest::Url::parse(&endpoint).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("AWS IMDS endpoint is invalid: {error}"),
        })?;
        base.join(path).map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: format!("AWS IMDS metadata path is invalid: {error}"),
        })
    }
}

#[derive(Debug, Deserialize)]
struct AwsImdsRoleCredentials {
    #[serde(rename = "Code")]
    code: Option<String>,
    #[serde(rename = "AccessKeyId")]
    access_key_id: Option<String>,
    #[serde(rename = "SecretAccessKey")]
    secret_access_key: Option<String>,
    #[serde(rename = "Token")]
    session_token: Option<String>,
}

impl AwsImdsRoleCredentials {
    fn into_credential(self, provider: &str, region: &str) -> Result<Credential, ProviderError> {
        if let Some(code) = self.code
            && code != "Success"
        {
            return Err(ProviderError::InvalidResponse {
                provider: provider.to_string(),
                message: format!("AWS IMDS role credential response returned Code `{code}`"),
            });
        }

        let access_key_id = required_field(provider, "AccessKeyId", self.access_key_id)?;
        let secret_access_key =
            required_field(provider, "SecretAccessKey", self.secret_access_key)?;
        let session_token = required_field(provider, "Token", self.session_token)?;

        Ok(Credential::aws_sigv4(
            access_key_id,
            secret_access_key,
            Some(session_token.as_str()),
            region,
        ))
    }
}

fn required_field(
    provider: &str,
    name: &str,
    value: Option<String>,
) -> Result<String, ProviderError> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ProviderError::InvalidResponse {
            provider: provider.to_string(),
            message: format!("AWS IMDS role credential response did not include {name}"),
        }),
    }
}

fn aws_managed_identity_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build proxy-free AWS managed identity HTTP client")
}
