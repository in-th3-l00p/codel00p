use futures::StreamExt;

use crate::ProviderError;

/// Streams a server-sent-event response body, invoking `on_data` with the
/// payload of each `data:` line as it arrives. A line whose payload is `[DONE]`
/// ends the stream, and `on_data` may return `Ok(true)` to stop early.
///
/// The caller is responsible for sending the request; this helper validates the
/// HTTP status (reading the body for error detail) before decoding events.
pub(crate) async fn stream_sse<F>(
    provider: &str,
    response: reqwest::Response,
    mut on_data: F,
) -> Result<(), ProviderError>
where
    F: FnMut(&str) -> Result<bool, ProviderError>,
{
    let status = response.status();
    if !status.is_success() {
        let retry_after = super::retry_after_seconds(response.headers());
        let body = response.text().await.unwrap_or_default();
        return Err(super::http_status_error(
            provider,
            status,
            &body,
            retry_after,
        ));
    }

    let mut buffer = String::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|error| ProviderError::Http {
            provider: provider.to_string(),
            message: error.to_string(),
        })?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline) = buffer.find('\n') {
            let line: String = buffer.drain(..=newline).collect();
            if let Some(data) = parse_sse_data_line(line.trim_end()) {
                if data == "[DONE]" {
                    return Ok(());
                }
                if on_data(data)? {
                    return Ok(());
                }
            }
        }
    }

    // Flush a trailing line that arrived without a terminating newline.
    if let Some(data) = parse_sse_data_line(buffer.trim_end())
        && data != "[DONE]"
    {
        on_data(data)?;
    }

    Ok(())
}

/// Returns the payload of an SSE `data:` line, or `None` for blank or
/// non-`data:` lines (comments, `event:` lines, blank separators).
pub(crate) fn parse_sse_data_line(line: &str) -> Option<&str> {
    let payload = line.strip_prefix("data:")?;
    let payload = payload.trim();
    if payload.is_empty() {
        None
    } else {
        Some(payload)
    }
}
