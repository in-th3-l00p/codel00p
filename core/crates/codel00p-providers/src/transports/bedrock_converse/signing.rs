//! AWS Signature Version 4 signing for Bedrock runtime requests.

use super::*;

pub(super) fn sign_bedrock_request(
    url: &reqwest::Url,
    body: &[u8],
    access_key_id: &str,
    secret_access_key: &str,
    session_token: Option<&str>,
    region: &str,
) -> Result<HeaderMap, ProviderError> {
    let now = OffsetDateTime::now_utc();
    let date_stamp = format!(
        "{:04}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    );
    let amz_date = format!(
        "{date_stamp}T{:02}{:02}{:02}Z",
        now.hour(),
        now.minute(),
        now.second()
    );
    let payload_hash = sha256_hex(body);
    let host = canonical_host(url);

    let mut signed_headers = vec![
        ("content-type", "application/json".to_string()),
        ("host", host.clone()),
        ("x-amz-content-sha256", payload_hash.clone()),
        ("x-amz-date", amz_date.clone()),
    ];
    if let Some(session_token) = session_token {
        signed_headers.push(("x-amz-security-token", session_token.to_string()));
    }
    signed_headers.sort_by_key(|(name, _)| *name);

    let canonical_headers = signed_headers
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect::<String>();
    let signed_header_names = signed_headers
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "POST\n{}\n\n{}{}\n{}",
        url.path(),
        canonical_headers,
        signed_header_names,
        payload_hash
    );
    let credential_scope = format!("{date_stamp}/{region}/{BEDROCK_SERVICE}/aws4_request");
    let string_to_sign = format!(
        "{AWS_ALGORITHM}\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = aws_signing_key(secret_access_key, &date_stamp, region, BEDROCK_SERVICE);
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "{AWS_ALGORITHM} Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_header_names}, Signature={signature}"
    );

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        HOST,
        HeaderValue::from_str(&host).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );
    headers.insert(
        HeaderName::from_static("x-amz-content-sha256"),
        HeaderValue::from_str(&payload_hash).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );
    headers.insert(
        HeaderName::from_static("x-amz-date"),
        HeaderValue::from_str(&amz_date).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );
    if let Some(session_token) = session_token {
        headers.insert(
            HeaderName::from_static("x-amz-security-token"),
            HeaderValue::from_str(session_token).map_err(|error| ProviderError::Http {
                provider: "bedrock".to_string(),
                message: error.to_string(),
            })?,
        );
    }
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&authorization).map_err(|error| ProviderError::Http {
            provider: "bedrock".to_string(),
            message: error.to_string(),
        })?,
    );

    Ok(headers)
}

fn canonical_host(url: &reqwest::Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn aws_signing_key(
    secret_access_key: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
) -> Vec<u8> {
    let date_key = hmac_sha256(
        format!("AWS4{secret_access_key}").as_bytes(),
        date_stamp.as_bytes(),
    );
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, service.as_bytes());
    hmac_sha256(&service_key, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}
