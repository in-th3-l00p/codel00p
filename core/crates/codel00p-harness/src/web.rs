//! Built-in web tools: `web_fetch` and `web_search`.
//!
//! These tools require [`PermissionScope::Network`] because they reach hosts
//! outside the workspace. They are intentionally self-contained: `web_fetch`
//! performs a single GET and renders HTML down to readable text, while
//! `web_search` proxies a pluggable search backend selected by environment
//! variables.

use std::time::Duration;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

/// Request timeout for both web tools.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Maximum number of bytes we will read from a `web_fetch` response body.
const MAX_RESPONSE_BYTES: usize = 1_048_576; // ~1 MiB
/// Default and ceiling for `web_search` result counts.
const DEFAULT_SEARCH_RESULTS: usize = 5;
const MAX_SEARCH_RESULTS: usize = 20;

/// Environment variable holding the Brave Search API key.
const BRAVE_API_KEY_ENV: &str = "BRAVE_SEARCH_API_KEY";
/// Default Brave Search endpoint; override with `CODEL00P_SEARCH_API_URL`.
const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const SEARCH_API_URL_ENV: &str = "CODEL00P_SEARCH_API_URL";

/// Build the registry fragment that exposes the web tools.
pub fn web_tools() -> crate::ToolRegistry {
    crate::ToolRegistry::new()
        .with_tool(WebFetchTool)
        .with_tool(WebSearchTool::from_env())
}

/// Fetch a single URL and return readable text.
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL over HTTP(S) and return it as readable text. HTML is \
         stripped to plain text; JSON and plain text pass through unchanged."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": { "type": "string" }
            }
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::Network
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let url = required_string(self.name(), &input, "url")?;
        validate_http_url(self.name(), url)?;

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| self.failed(error.to_string()))?;

        let response = client.get(url).send().await.map_err(|error| {
            let message = if error.is_timeout() {
                format!(
                    "request to `{url}` timed out after {}s",
                    REQUEST_TIMEOUT.as_secs()
                )
            } else {
                format!("request to `{url}` failed: {error}")
            };
            self.failed(message)
        })?;

        let status = response.status();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();

        if !status.is_success() {
            return Err(self.failed(format!(
                "fetch of `{final_url}` returned non-success status {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            )));
        }

        let body = read_capped_body(self.name(), response).await?;
        let text = render_body(&content_type, &body);

        Ok(ToolResult::json(json!({
            "url": final_url,
            "status": status.as_u16(),
            "content_type": content_type,
            "text": text,
        })))
    }
}

impl WebFetchTool {
    fn failed(&self, message: String) -> HarnessError {
        HarnessError::ToolFailed {
            name: self.name().to_string(),
            message,
        }
    }
}

/// Search the web through a pluggable backend.
pub struct WebSearchTool {
    backend: SearchBackend,
}

/// Backend configuration resolved from the environment.
enum SearchBackend {
    /// A Brave-compatible search API with an endpoint and API key.
    Brave { endpoint: String, api_key: String },
    /// No usable configuration was found in the environment.
    Unconfigured,
}

impl WebSearchTool {
    /// Resolve the backend from the process environment.
    ///
    /// Set `BRAVE_SEARCH_API_KEY` to enable search. Optionally override the
    /// endpoint with `CODEL00P_SEARCH_API_URL` (must speak the Brave Search
    /// web-results response shape).
    pub fn from_env() -> Self {
        let backend = match std::env::var(BRAVE_API_KEY_ENV) {
            Ok(api_key) if !api_key.trim().is_empty() => {
                let endpoint = std::env::var(SEARCH_API_URL_ENV)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| BRAVE_SEARCH_URL.to_string());
                SearchBackend::Brave { endpoint, api_key }
            }
            _ => SearchBackend::Unconfigured,
        };
        Self { backend }
    }

    fn failed(&self, message: String) -> HarnessError {
        HarnessError::ToolFailed {
            name: self.name().to_string(),
            message,
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web and return a list of { title, url, snippet } results. \
         Requires a configured search backend: set BRAVE_SEARCH_API_KEY to a \
         Brave Search API key. To use a self-hosted or Brave-compatible proxy \
         instead of Brave's default endpoint, also set CODEL00P_SEARCH_API_URL \
         to that endpoint's URL."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string" },
                "max_results": { "type": "integer", "minimum": 1, "maximum": MAX_SEARCH_RESULTS }
            }
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::Network
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let query = required_string(self.name(), &input, "query")?;
        if query.trim().is_empty() {
            return Err(HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "`query` must not be empty".to_string(),
            });
        }
        let max_results = input
            .get("max_results")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_SEARCH_RESULTS)
            .clamp(1, MAX_SEARCH_RESULTS);

        let (endpoint, api_key) = match &self.backend {
            SearchBackend::Brave { endpoint, api_key } => (endpoint, api_key),
            SearchBackend::Unconfigured => {
                return Err(self.failed(format!(
                    "web_search is not configured: set the `{BRAVE_API_KEY_ENV}` environment \
                     variable to a Brave Search API key (see https://brave.com/search/api/ to \
                     obtain one). To point at a self-hosted or Brave-compatible endpoint instead \
                     of Brave's default, also set `{SEARCH_API_URL_ENV}` to that endpoint's URL."
                )));
            }
        };

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| self.failed(error.to_string()))?;

        let response = client
            .get(endpoint)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", &max_results.to_string())])
            .send()
            .await
            .map_err(|error| {
                let message = if error.is_timeout() {
                    format!(
                        "search request timed out after {}s",
                        REQUEST_TIMEOUT.as_secs()
                    )
                } else {
                    format!("search request failed: {error}")
                };
                self.failed(message)
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.failed(format!(
                "search backend returned non-success status {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            )));
        }

        let payload: Value = response
            .json()
            .await
            .map_err(|error| self.failed(format!("could not parse search response: {error}")))?;
        let results = parse_brave_results(&payload, max_results);

        Ok(ToolResult::json(json!({
            "query": query,
            "results": results,
        })))
    }
}

/// Reject anything that is not an absolute `http`/`https` URL.
fn validate_http_url(tool: &str, url: &str) -> Result<(), HarnessError> {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(())
    } else {
        Err(HarnessError::InvalidToolInput {
            name: tool.to_string(),
            message: format!("`url` must be an http(s) URL, got `{url}`"),
        })
    }
}

/// Read the response body, returning an error if it exceeds the size cap.
async fn read_capped_body(
    tool: &str,
    response: reqwest::Response,
) -> Result<Vec<u8>, HarnessError> {
    use futures::StreamExt;

    if let Some(length) = response.content_length()
        && length as usize > MAX_RESPONSE_BYTES
    {
        return Err(HarnessError::ToolFailed {
            name: tool.to_string(),
            message: format!(
                "response body of {length} bytes exceeds the {MAX_RESPONSE_BYTES} byte cap"
            ),
        });
    }

    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| HarnessError::ToolFailed {
            name: tool.to_string(),
            message: format!("error reading response body: {error}"),
        })?;
        body.extend_from_slice(&chunk);
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(HarnessError::ToolFailed {
                name: tool.to_string(),
                message: format!("response body exceeds the {MAX_RESPONSE_BYTES} byte cap"),
            });
        }
    }

    Ok(body)
}

/// Render a response body to text based on its content type.
fn render_body(content_type: &str, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let lower = content_type.to_ascii_lowercase();
    if lower.contains("text/html") || lower.contains("application/xhtml") {
        html_to_text(&text)
    } else {
        collapse_whitespace(&text)
    }
}

/// Convert HTML markup into readable plain text.
///
/// This is a deliberately small, dependency-free pass: it drops the contents
/// of `<script>`/`<style>` blocks, strips remaining tags, decodes a handful of
/// common entities, and collapses runs of whitespace. It is good enough to
/// hand an LLM the gist of a page without pulling in a full HTML parser.
fn html_to_text(html: &str) -> String {
    let without_blocks = strip_blocks(html);
    let mut out = String::with_capacity(without_blocks.len());
    let mut in_tag = false;
    for ch in without_blocks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            _ => out.push(ch),
        }
    }
    let decoded = decode_entities(&out);
    collapse_whitespace(&decoded)
}

/// Remove `<script>`/`<style>` elements (including their text) entirely.
fn strip_blocks(html: &str) -> String {
    let mut result = html.to_string();
    for tag in ["script", "style"] {
        result = strip_element(&result, tag);
    }
    result
}

fn strip_element(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(start) = lower[cursor..].find(&open) {
        let start = cursor + start;
        out.push_str(&html[cursor..start]);
        match lower[start..].find(&close) {
            Some(end) => cursor = start + end + close.len(),
            None => {
                cursor = html.len();
                break;
            }
        }
    }
    out.push_str(&html[cursor..]);
    out
}

/// Decode the small set of HTML entities that show up in readable text.
fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Collapse consecutive whitespace into single spaces and trim the ends.
fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract `{ title, url, snippet }` results from a Brave-style payload.
fn parse_brave_results(payload: &Value, max_results: usize) -> Vec<Value> {
    payload
        .get("web")
        .and_then(|web| web.get("results"))
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .take(max_results)
                .map(|result| {
                    json!({
                        "title": result.get("title").and_then(Value::as_str).unwrap_or(""),
                        "url": result.get("url").and_then(Value::as_str).unwrap_or(""),
                        "snippet": result
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn workspace() -> Workspace {
        let dir = tempfile::tempdir().unwrap();
        Workspace::new(dir.path()).unwrap()
    }

    #[test]
    fn html_to_text_strips_tags_scripts_and_collapses() {
        let html = "<html><head><style>p{color:red}</style>\
            <script>alert('x')</script></head><body>\
            <h1>Hello</h1>  <p>World &amp; more</p></body></html>";
        let text = html_to_text(html);
        assert_eq!(text, "Hello World & more");
        assert!(!text.contains("alert"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn validate_http_url_rejects_non_http() {
        assert!(validate_http_url("web_fetch", "ftp://example.com").is_err());
        assert!(validate_http_url("web_fetch", "file:///etc/passwd").is_err());
        assert!(validate_http_url("web_fetch", "https://example.com").is_ok());
    }

    #[tokio::test]
    async fn web_fetch_renders_html_to_text() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/page");
                then.status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body("<html><body><h1>Title</h1><p>Body text</p></body></html>");
            })
            .await;

        let result = WebFetchTool
            .execute(&workspace(), json!({ "url": server.url("/page") }))
            .await
            .unwrap();

        mock.assert_async().await;
        let content = result.content();
        assert_eq!(content["status"], 200);
        assert_eq!(content["text"], "Title Body text");
    }

    #[tokio::test]
    async fn web_fetch_passes_json_through() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/data");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(r#"{"a":1}"#);
            })
            .await;

        let result = WebFetchTool
            .execute(&workspace(), json!({ "url": server.url("/data") }))
            .await
            .unwrap();

        assert_eq!(result.content()["text"], r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn web_fetch_follows_redirect_and_reports_final_url() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/start");
                then.status(302).header("location", "/dest");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/dest");
                then.status(200)
                    .header("content-type", "text/plain")
                    .body("arrived");
            })
            .await;

        let result = WebFetchTool
            .execute(&workspace(), json!({ "url": server.url("/start") }))
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["text"], "arrived");
        assert!(content["url"].as_str().unwrap().ends_with("/dest"));
    }

    #[tokio::test]
    async fn web_fetch_errors_on_non_success_status() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/missing");
                then.status(404).body("nope");
            })
            .await;

        let error = WebFetchTool
            .execute(&workspace(), json!({ "url": server.url("/missing") }))
            .await
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("404"), "unexpected error: {message}");
    }

    #[tokio::test]
    async fn web_fetch_errors_when_body_exceeds_cap() {
        let server = MockServer::start_async().await;
        let oversized = "a".repeat(MAX_RESPONSE_BYTES + 1024);
        server
            .mock_async(|when, then| {
                when.method(GET).path("/big");
                then.status(200)
                    .header("content-type", "text/plain")
                    .body(oversized);
            })
            .await;

        let error = WebFetchTool
            .execute(&workspace(), json!({ "url": server.url("/big") }))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("cap"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn web_search_returns_results_from_backend() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/search").query_param("q", "rust");
                then.status(200).header("content-type", "application/json").json_body(json!({
                    "web": {
                        "results": [
                            { "title": "Rust", "url": "https://rust-lang.org", "description": "A language" },
                            { "title": "Docs", "url": "https://doc.rust-lang.org", "description": "Docs" }
                        ]
                    }
                }));
            })
            .await;

        let tool = WebSearchTool {
            backend: SearchBackend::Brave {
                endpoint: server.url("/search"),
                api_key: "test-key".to_string(),
            },
        };

        let result = tool
            .execute(&workspace(), json!({ "query": "rust", "max_results": 5 }))
            .await
            .unwrap();

        mock.assert_async().await;
        let results = result.content()["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["title"], "Rust");
        assert_eq!(results[0]["url"], "https://rust-lang.org");
        assert_eq!(results[0]["snippet"], "A language");
    }

    #[tokio::test]
    async fn web_search_unconfigured_returns_actionable_error() {
        let tool = WebSearchTool {
            backend: SearchBackend::Unconfigured,
        };

        let error = tool
            .execute(&workspace(), json!({ "query": "rust" }))
            .await
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains(BRAVE_API_KEY_ENV), "unexpected: {message}");
        assert!(message.contains("not configured"), "unexpected: {message}");
    }
}
