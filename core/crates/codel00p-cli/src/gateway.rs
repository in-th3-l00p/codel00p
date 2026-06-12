//! The `codel00p gateway` command: reach one agent core from chat platforms.
//!
//! `gateway message` is the per-message entrypoint a platform adapter calls;
//! `gateway serve` is a minimal HTTP webhook (POST /message) that platform event
//! subscriptions (Slack, Telegram, …) post to. Both map a conversation to a
//! durable agent session and run the message as a turn.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
};

use serde::Deserialize;
use serde_json::json;

use crate::{
    config::{CliConfig, CliResult},
    settings::AgentSettings,
};

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
    let listener = TcpListener::bind((options.bind.as_str(), options.port))
        .map_err(|error| format!("failed to bind {}:{}: {error}", options.bind, options.port))?;
    let addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{}:{}", options.bind, options.port));
    eprintln!("codel00p gateway listening on http://{addr} (POST /message, Ctrl-C to stop)");
    serve_loop(listener, config, defaults);
    Ok(String::new())
}

/// Accept connections forever, handling one request each. Sequential by design:
/// agent turns for the same conversation must not race on its session.
pub(crate) fn serve_loop(listener: TcpListener, config: CliConfig, defaults: AgentSettings) {
    for stream in listener.incoming().flatten() {
        if let Err(error) = handle_connection(stream, &config, &defaults) {
            eprintln!("gateway connection error: {error}");
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    config: &CliConfig,
    defaults: &AgentSettings,
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
        if let Some(value) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body);

    let (status, payload) = dispatch(config, defaults, &method, &path, &body);
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
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String) {
    match (method, path) {
        ("GET", "/healthz") => (200, json!({ "status": "ok" }).to_string()),
        ("POST", "/message") => message_reply(config, defaults, body),
        _ => (404, json!({ "error": "not found" }).to_string()),
    }
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

    #[test]
    fn dispatch_routes_health_unknown_and_bad_json() {
        let dir = tempdir().unwrap();
        let config = test_config(dir.path());
        let defaults = AgentSettings::default();

        assert_eq!(dispatch(&config, &defaults, "GET", "/healthz", "").0, 200);
        assert_eq!(dispatch(&config, &defaults, "GET", "/nope", "").0, 404);
        // Invalid JSON is a 400 before any agent run, so no provider is needed.
        assert_eq!(
            dispatch(&config, &defaults, "POST", "/message", "not json").0,
            400
        );
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
        std::thread::spawn(move || serve_loop(listener, config, defaults));

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
