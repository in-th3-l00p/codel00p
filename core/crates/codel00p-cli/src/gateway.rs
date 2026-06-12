//! The `codel00p gateway` command: reach one agent core from chat platforms.
//!
//! `gateway message` is the per-message entrypoint a platform adapter calls;
//! `gateway serve` is an HTTP webhook that platform event subscriptions post to:
//!
//! - `POST /message` — generic JSON `{conversation, user, text}`, protected by a
//!   shared bearer secret (`CODEL00P_GATEWAY_SECRET`).
//! - `POST /slack/events` — the Slack Events API, authenticated by Slack request
//!   signing (`CODEL00P_SLACK_SIGNING_SECRET`); answers the URL-verification
//!   handshake and runs message events as turns.
//! - `GET /healthz` — liveness, always open.
//!
//! All map a conversation to a durable agent session and run the message as a
//! turn. Privileged tools pause for the remote user's `/approve` (see
//! [`crate::agent`]).

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    time::{SystemTime, UNIX_EPOCH},
};

use codel00p_gateway::{
    auth::authorize,
    slack::{SlackEvent, parse_slack_event},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;

use crate::{
    config::{CliConfig, CliResult},
    settings::AgentSettings,
};

/// Shared secrets that protect the webhook, read once at `gateway serve` start.
#[derive(Clone, Default)]
pub(crate) struct GatewaySecrets {
    /// Bearer token required on generic webhook routes (`POST /message`). When
    /// `None`, those routes are open — trusted/local use only.
    webhook: Option<String>,
    /// Slack request-signing secret. When set, `POST /slack/events` requests are
    /// verified against their `X-Slack-Signature`; when `None`, Slack events are
    /// accepted unverified — trusted/local use only.
    slack_signing: Option<String>,
}

impl GatewaySecrets {
    fn from_env() -> Self {
        let read = |key: &str| std::env::var(key).ok().filter(|value| !value.is_empty());
        Self {
            webhook: read("CODEL00P_GATEWAY_SECRET"),
            slack_signing: read("CODEL00P_SLACK_SIGNING_SECRET"),
        }
    }
}

pub fn run(config: CliConfig, defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => return Err("usage: codel00p gateway <message|serve> ...".to_string()),
    };
    match command {
        "message" => gateway_message(config, &defaults, rest),
        "serve" => gateway_serve(config, defaults, rest),
        _ => Err(format!("unknown gateway command: {command}")),
    }
}

// --- HTTP webhook (`gateway serve`) ----------------------------------------

struct ServeOptions {
    bind: String,
    port: u16,
}

fn parse_serve(args: &[String]) -> CliResult<ServeOptions> {
    let mut bind = "127.0.0.1".to_string();
    let mut port = 8765u16;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--bind" => {
                bind = value_after(args, index, "--bind")?;
                index += 2;
            }
            "--port" => {
                port = value_after(args, index, "--port")?
                    .parse()
                    .map_err(|_| "--port must be a number".to_string())?;
                index += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown gateway serve option: {flag}"));
            }
            value => return Err(format!("unexpected argument: {value}")),
        }
    }
    Ok(ServeOptions { bind, port })
}

fn gateway_serve(config: CliConfig, defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    let options = parse_serve(args)?;
    let secrets = GatewaySecrets::from_env();
    let listener = TcpListener::bind((options.bind.as_str(), options.port))
        .map_err(|error| format!("failed to bind {}:{}: {error}", options.bind, options.port))?;
    let addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{}:{}", options.bind, options.port));
    if secrets.webhook.is_none() {
        eprintln!(
            "warning: CODEL00P_GATEWAY_SECRET not set; POST /message accepts all callers (trusted/local only)"
        );
    }
    if secrets.slack_signing.is_none() {
        eprintln!(
            "warning: CODEL00P_SLACK_SIGNING_SECRET not set; POST /slack/events is unverified (trusted/local only)"
        );
    }
    eprintln!(
        "codel00p gateway listening on http://{addr} (POST /message, POST /slack/events, Ctrl-C to stop)"
    );
    serve_loop(listener, config, defaults, secrets);
    Ok(String::new())
}

/// Accept connections forever, handling one request each. Sequential by design:
/// agent turns for the same conversation must not race on its session.
pub(crate) fn serve_loop(
    listener: TcpListener,
    config: CliConfig,
    defaults: AgentSettings,
    secrets: GatewaySecrets,
) {
    for stream in listener.incoming().flatten() {
        if let Err(error) = handle_connection(stream, &config, &defaults, &secrets) {
            eprintln!("gateway connection error: {error}");
        }
    }
}

/// One parsed inbound HTTP request: method, path, headers (names lowercased),
/// and the raw body.
struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

fn handle_connection(
    mut stream: TcpStream,
    config: &CliConfig,
    defaults: &AgentSettings,
    secrets: &GatewaySecrets,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("")
        .to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let request = HttpRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };

    let (status, payload) = dispatch(config, defaults, secrets, &request);
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{payload}",
        reason = status_reason(status),
        len = payload.len(),
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn dispatch(
    config: &CliConfig,
    defaults: &AgentSettings,
    secrets: &GatewaySecrets,
    request: &HttpRequest,
) -> (u16, String) {
    match (request.method.as_str(), request.path.as_str()) {
        // Liveness probes are always open.
        ("GET", "/healthz") => (200, json!({ "status": "ok" }).to_string()),
        // Slack authenticates with its own request signature, not the bearer.
        ("POST", "/slack/events") => slack_reply(config, defaults, secrets, request),
        // Generic webhook routes require the shared bearer secret (if set).
        ("POST", "/message") => {
            if !authorize(secrets.webhook.as_deref(), request.header("authorization")) {
                return (401, json!({ "error": "unauthorized" }).to_string());
            }
            message_reply(config, defaults, &request.body)
        }
        _ => (404, json!({ "error": "not found" }).to_string()),
    }
}

/// Handle a Slack Events API callback: verify the request signature, answer the
/// URL-verification handshake, and run message events as gateway turns.
fn slack_reply(
    config: &CliConfig,
    defaults: &AgentSettings,
    secrets: &GatewaySecrets,
    request: &HttpRequest,
) -> (u16, String) {
    if let Some(signing_secret) = secrets.slack_signing.as_deref()
        && !slack_signature_valid(signing_secret, request)
    {
        return (
            401,
            json!({ "error": "invalid slack signature" }).to_string(),
        );
    }
    match parse_slack_event(&request.body) {
        Ok(SlackEvent::UrlVerification { challenge }) => {
            (200, json!({ "challenge": challenge }).to_string())
        }
        Ok(SlackEvent::Message {
            conversation,
            user,
            text,
        }) => match crate::agent::run_gateway_message(
            config.clone(),
            defaults,
            &conversation,
            &user,
            &text,
        ) {
            Ok(reply) => (200, json!({ "reply": reply.trim_end() }).to_string()),
            Err(error) => (500, json!({ "error": error }).to_string()),
        },
        // Bot echoes, edits, and non-message events are acknowledged and ignored.
        Ok(SlackEvent::Ignored) => (200, json!({ "status": "ignored" }).to_string()),
        Err(error) => (
            400,
            json!({ "error": format!("invalid slack event: {error}") }).to_string(),
        ),
    }
}

/// Verify a Slack request signature: `HMAC-SHA256("v0:{ts}:{body}")` keyed by
/// the signing secret must equal the `X-Slack-Signature` header, and the
/// timestamp must be recent (replay protection).
fn slack_signature_valid(signing_secret: &str, request: &HttpRequest) -> bool {
    let timestamp = match request.header("x-slack-request-timestamp") {
        Some(value) => value,
        None => return false,
    };
    let provided = match request.header("x-slack-signature") {
        Some(value) => value,
        None => return false,
    };
    // Reject stale timestamps (> 5 minutes) to blunt replay attacks.
    if let (Ok(ts), Ok(now)) = (
        timestamp.parse::<u64>(),
        SystemTime::now().duration_since(UNIX_EPOCH),
    ) {
        if now.as_secs().abs_diff(ts) > 60 * 5 {
            return false;
        }
    } else {
        return false;
    }
    let expected_hex = match provided.strip_prefix("v0=") {
        Some(hex) => hex,
        None => return false,
    };
    let expected = match decode_hex(expected_hex) {
        Some(bytes) => bytes,
        None => return false,
    };
    let basestring = format!("v0:{timestamp}:{}", request.body);
    let mut mac = match Hmac::<Sha256>::new_from_slice(signing_secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(basestring.as_bytes());
    // `verify_slice` is a constant-time comparison.
    mac.verify_slice(&expected).is_ok()
}

/// Decode a lowercase/uppercase hex string into bytes, or `None` if malformed.
fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

#[derive(Deserialize)]
struct MessageRequest {
    conversation: String,
    user: String,
    text: String,
}

fn message_reply(config: &CliConfig, defaults: &AgentSettings, body: &str) -> (u16, String) {
    let request: MessageRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return (
                400,
                json!({ "error": format!("invalid JSON: {error}") }).to_string(),
            );
        }
    };
    match crate::agent::run_gateway_message(
        config.clone(),
        defaults,
        &request.conversation,
        &request.user,
        &request.text,
    ) {
        Ok(reply) => (200, json!({ "reply": reply.trim_end() }).to_string()),
        Err(error) => (500, json!({ "error": error }).to_string()),
    }
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

struct MessageOptions {
    conversation: String,
    user: String,
    text: String,
}

fn parse_message(args: &[String]) -> CliResult<MessageOptions> {
    let mut conversation = None;
    let mut user = None;
    let mut text = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--conversation" => {
                conversation = Some(value_after(args, index, "--conversation")?);
                index += 2;
            }
            "--user" => {
                user = Some(value_after(args, index, "--user")?);
                index += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown gateway message option: {flag}"));
            }
            value => {
                text.push(value.to_string());
                index += 1;
            }
        }
    }

    let usage = "usage: gateway message --conversation <id> --user <id> <text>";
    let text = text.join(" ");
    if text.trim().is_empty() {
        return Err(usage.to_string());
    }
    Ok(MessageOptions {
        conversation: conversation.ok_or(usage)?,
        user: user.ok_or(usage)?,
        text,
    })
}

fn gateway_message(
    config: CliConfig,
    defaults: &AgentSettings,
    args: &[String],
) -> CliResult<String> {
    let options = parse_message(args)?;
    crate::agent::run_gateway_message(
        config,
        defaults,
        &options.conversation,
        &options.user,
        &options.text,
    )
}

fn value_after(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use codel00p_protocol::ProjectRef;
    use httpmock::{Method::POST, MockServer};
    use tempfile::tempdir;

    use crate::config::CliConfig;

    fn test_config(dir: &std::path::Path) -> CliConfig {
        CliConfig {
            memory_db: dir.join("memory.sqlite"),
            organization_id: "test".to_string(),
            project: ProjectRef::new("p", "P"),
        }
    }

    fn provider_defaults(base_url: &str) -> AgentSettings {
        AgentSettings {
            provider: Some("custom".to_string()),
            model: Some("test-model".to_string()),
            base_url: Some(base_url.to_string()),
            ..Default::default()
        }
    }

    fn req(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> HttpRequest {
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
                .collect(),
            body: body.to_string(),
        }
    }

    #[test]
    fn dispatch_routes_health_unknown_and_bad_json() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();
        let open = GatewaySecrets::default();

        assert_eq!(
            dispatch(&config, &defaults, &open, &req("GET", "/healthz", &[], "")).0,
            200
        );
        assert_eq!(
            dispatch(&config, &defaults, &open, &req("GET", "/nope", &[], "")).0,
            404
        );
        // Invalid JSON is a 400 before any agent run, so no provider is needed.
        assert_eq!(
            dispatch(
                &config,
                &defaults,
                &open,
                &req("POST", "/message", &[], "not json")
            )
            .0,
            400
        );
    }

    #[test]
    fn message_route_enforces_bearer_secret() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();
        let secured = GatewaySecrets {
            webhook: Some("s3cret".to_string()),
            slack_signing: None,
        };

        // No / wrong bearer -> 401, before any body parsing.
        assert_eq!(
            dispatch(
                &config,
                &defaults,
                &secured,
                &req("POST", "/message", &[], "not json")
            )
            .0,
            401
        );
        assert_eq!(
            dispatch(
                &config,
                &defaults,
                &secured,
                &req(
                    "POST",
                    "/message",
                    &[("Authorization", "Bearer nope")],
                    "not json"
                )
            )
            .0,
            401
        );
        // Correct bearer passes the gate; the bad JSON then yields 400.
        assert_eq!(
            dispatch(
                &config,
                &defaults,
                &secured,
                &req(
                    "POST",
                    "/message",
                    &[("Authorization", "Bearer s3cret")],
                    "not json"
                )
            )
            .0,
            400
        );
    }

    #[test]
    fn slack_url_verification_echoes_challenge() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();
        let open = GatewaySecrets::default();

        let body = r#"{"type":"url_verification","challenge":"abc123"}"#;
        let (status, payload) = dispatch(
            &config,
            &defaults,
            &open,
            &req("POST", "/slack/events", &[], body),
        );
        assert_eq!(status, 200);
        assert!(payload.contains("abc123"), "payload: {payload}");
    }

    #[test]
    fn slack_route_rejects_bad_signature() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();
        let secured = GatewaySecrets {
            webhook: None,
            slack_signing: Some("shhh".to_string()),
        };

        let body = r#"{"type":"url_verification","challenge":"abc123"}"#;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let headers = [
            ("X-Slack-Request-Timestamp", now.as_str()),
            ("X-Slack-Signature", "v0=deadbeef"),
        ];
        let (status, _) = dispatch(
            &config,
            &defaults,
            &secured,
            &req("POST", "/slack/events", &headers, body),
        );
        assert_eq!(status, 401);
    }

    #[test]
    fn slack_route_accepts_valid_signature() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();
        let secret = "shhh";
        let secured = GatewaySecrets {
            webhook: None,
            slack_signing: Some(secret.to_string()),
        };

        let body = r#"{"type":"url_verification","challenge":"abc123"}"#;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        // Sign exactly as Slack does: HMAC-SHA256("v0:{ts}:{body}").
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("v0:{timestamp}:{body}").as_bytes());
        let signature = format!(
            "v0={}",
            mac.finalize()
                .into_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let headers = [
            ("X-Slack-Request-Timestamp", timestamp.as_str()),
            ("X-Slack-Signature", signature.as_str()),
        ];
        let (status, payload) = dispatch(
            &config,
            &defaults,
            &secured,
            &req("POST", "/slack/events", &headers, body),
        );
        assert_eq!(status, 200, "payload: {payload}");
        assert!(payload.contains("abc123"), "payload: {payload}");
    }

    #[test]
    fn slack_ignores_bot_messages() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();
        let open = GatewaySecrets::default();

        // A bot echo (bot_id present) must be acknowledged but not run as a turn,
        // so no provider is required and the status is 200.
        let body = r#"{"type":"event_callback","event":{"type":"message","bot_id":"B1","channel":"C1","user":"U1","text":"hi"}}"#;
        let (status, payload) = dispatch(
            &config,
            &defaults,
            &open,
            &req("POST", "/slack/events", &[], body),
        );
        assert_eq!(status, 200, "payload: {payload}");
        assert!(payload.contains("ignored"), "payload: {payload}");
    }

    fn http_request(port: u16, raw: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(raw.as_bytes()).expect("write");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read");
        response
    }

    #[test]
    fn serve_handles_real_http_requests() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(json!({
                "choices": [
                    { "message": { "role": "assistant", "content": "hello from the gateway" }, "finish_reason": "stop" }
                ]
            }));
        });

        let dir = tempdir().unwrap();
        // SAFETY: guarded by LOCK; the CLI suite runs single-threaded.
        unsafe {
            std::env::set_var("CODEL00P_PROVIDER_CUSTOM_API_KEY", "test-token");
        }

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = test_config(dir.path());
        let defaults = provider_defaults(&server.base_url());
        std::thread::spawn(move || {
            serve_loop(listener, config, defaults, GatewaySecrets::default())
        });

        let health = http_request(
            port,
            "GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(health.contains("200 OK"), "health: {health}");
        assert!(health.contains("\"status\":\"ok\""), "health: {health}");

        let body = r#"{"conversation":"c-http","user":"u1","text":"hi there"}"#;
        let request = format!(
            "POST /message HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let response = http_request(port, &request);

        // SAFETY: still under LOCK.
        unsafe {
            std::env::remove_var("CODEL00P_PROVIDER_CUSTOM_API_KEY");
        }

        assert!(response.contains("200 OK"), "resp: {response}");
        assert!(
            response.contains("hello from the gateway"),
            "resp: {response}"
        );
    }
}
