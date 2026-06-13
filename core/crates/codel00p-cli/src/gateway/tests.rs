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
    std::thread::spawn(move || serve_loop(listener, config, defaults, GatewaySecrets::default()));

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
