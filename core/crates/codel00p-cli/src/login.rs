use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::config::{CliResult, required_value};
use crate::credentials::{self, Credentials};

const DEFAULT_LOGIN_URL: &str = "http://localhost:3000/connect/cli";
const TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// `codel00p login` — browser-based sign-in. Opens the system browser to the web
/// handoff page, receives a session token on a localhost loopback, and stores it
/// so the `cloud` commands work without `--token`.
pub fn run_login(args: &[String]) -> CliResult<String> {
    let mut connect_url = env::var("CODEL00P_LOGIN_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOGIN_URL.to_string());
    let mut api_url = env::var("CODEL00P_API_URL")
        .ok()
        .filter(|v| !v.trim().is_empty());
    let mut org_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--connect-url" => {
                connect_url = required_value(args, index, "--connect-url")?;
                index += 2;
            }
            "--api-url" => {
                api_url = Some(required_value(args, index, "--api-url")?);
                index += 2;
            }
            "--org" | "--org-id" => {
                org_id = Some(required_value(args, index, args[index].as_str())?);
                index += 2;
            }
            flag => return Err(format!("unknown login option: {flag}")),
        }
    }

    let token = run_loopback(&connect_url, org_id.as_deref())?;
    let claims = decode_claims(&token).unwrap_or_default();
    let mut credentials = Credentials {
        token: Some(token),
        api_url,
        ..Credentials::default()
    };
    credentials.org_id = claims_str(&claims, "org_id");
    credentials.org_name = claims_str(&claims, "org_name");
    credentials.email = claims_str(&claims, "email");
    credentials::save(&credentials)?;

    let who = credentials
        .email
        .clone()
        .or_else(|| claims_str(&claims, "sub"))
        .unwrap_or_else(|| "your account".to_string());
    let org = credentials
        .org_name
        .clone()
        .or_else(|| credentials.org_id.clone());
    let mut output = format!("Logged in as {who}.\n");
    match org {
        Some(org) => output.push_str(&format!("Active organization: {org}\n")),
        None => output.push_str(
            "No active organization — select one in the web app, then `codel00p auth login` again.\n",
        ),
    }
    output.push_str(&format!(
        "Credentials saved to {}\n",
        credentials::credentials_path().display()
    ));
    Ok(output)
}

/// `codel00p logout` — clears stored credentials.
pub fn run_logout(args: &[String]) -> CliResult<String> {
    if !args.is_empty() {
        return Err(format!("unknown logout option: {}", args[0]));
    }
    if credentials::clear()? {
        Ok("Logged out.\n".to_string())
    } else {
        Ok("Not logged in.\n".to_string())
    }
}

/// Starts a localhost loopback, opens the browser to the handoff page (with the
/// port + a CSRF `state`), and waits for the redirect carrying the token.
fn run_loopback(connect_url: &str, org_id: Option<&str>) -> CliResult<String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = random_state();

    let target = login_target(connect_url, port, &state, org_id);
    open_browser(&target)?;
    println!("Opening your browser to sign in…");
    println!("If it doesn't open, visit:\n  {target}");

    let deadline = Instant::now() + TIMEOUT;
    loop {
        if Instant::now() >= deadline {
            return Err("login timed out".to_string());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let request_line = read_request_line(&mut stream);
                let (token, returned_state) = parse_callback(request_line.as_deref());
                let ok = token.is_some() && returned_state.as_deref() == Some(state.as_str());
                let _ = write_response(&mut stream, ok);
                if ok {
                    return Ok(token.unwrap());
                }
                // Ignore favicon/other probes and keep waiting.
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn login_target(connect_url: &str, port: u16, state: &str, org_id: Option<&str>) -> String {
    let mut target = format!("{connect_url}?port={port}&state={state}");
    if let Some(org_id) = org_id.filter(|value| !value.trim().is_empty()) {
        target.push_str("&org_id=");
        target.push_str(&url_encode(org_id));
    }
    target
}

fn read_request_line(stream: &mut std::net::TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    Some(line)
}

/// Extracts `token` and `state` from a `GET /callback?token=…&state=… HTTP/1.1`
/// request line.
fn parse_callback(request_line: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(line) = request_line else {
        return (None, None);
    };
    let Some(path) = line.split_whitespace().nth(1) else {
        return (None, None);
    };
    let Some(query) = path.strip_prefix("/callback?") else {
        return (None, None);
    };
    let mut token = None;
    let mut state = None;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "token" => token = Some(url_decode(value)),
                "state" => state = Some(url_decode(value)),
                _ => {}
            }
        }
    }
    (token, state)
}

fn write_response(stream: &mut std::net::TcpStream, success: bool) -> std::io::Result<()> {
    let body = result_page(success);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn decode_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn claims_str(claims: &serde_json::Value, key: &str) -> Option<String> {
    claims
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn random_state() -> String {
    // A unique, non-secret CSRF token: process id + monotonic nanos.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", std::process::id(), nanos)
}

pub(crate) fn open_browser(url: &str) -> CliResult<()> {
    let result = match env::consts::OS {
        "macos" => Command::new("open").arg(url).spawn(),
        "windows" => Command::new("cmd").args(["/C", "start", "", url]).spawn(),
        _ => Command::new("xdg-open").arg(url).spawn(),
    };
    result.map(|_| ()).map_err(|error| {
        format!("could not open a browser ({error}); visit the URL above manually")
    })
}

fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = (bytes[index + 1] as char).to_digit(16);
                let lo = (bytes[index + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn url_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn result_page(success: bool) -> String {
    let heading = if success {
        "You&rsquo;re signed in"
    } else {
        "Sign-in didn&rsquo;t complete"
    };
    let body = if success {
        "Return to your terminal — you can close this tab."
    } else {
        "Something went wrong. Return to the terminal and run codel00p auth login again."
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\" />\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" /><title>codel00p</title>\
<style>:root{{color-scheme:dark}}body{{margin:0;min-height:100vh;display:grid;place-items:center;\
background:#0c0a10;color:#f5f3f8;font-family:ui-sans-serif,system-ui,-apple-system,sans-serif;\
background-image:radial-gradient(60% 40% at 50% -8%,rgba(139,124,246,.22),transparent 70%)}}\
.card{{text-align:center;max-width:24rem;padding:2.5rem 2rem}}\
.dot{{width:12px;height:12px;border-radius:999px;background:#8b7cf6;box-shadow:0 0 18px #8b7cf6;margin:0 auto 1.25rem}}\
h1{{font-size:1.5rem;letter-spacing:-.02em;margin:0 0 .5rem}}p{{color:#a59fb3;line-height:1.6;margin:0}}</style></head>\
<body><div class=\"card\"><div class=\"dot\"></div><h1>{heading}</h1><p>{body}</p></div></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_callback_query() {
        let (token, state) = parse_callback(Some(
            "GET /callback?token=abc.def.ghi&state=xyz HTTP/1.1\r\n",
        ));
        assert_eq!(token.as_deref(), Some("abc.def.ghi"));
        assert_eq!(state.as_deref(), Some("xyz"));
    }

    #[test]
    fn ignores_non_callback_paths() {
        let (token, state) = parse_callback(Some("GET /favicon.ico HTTP/1.1\r\n"));
        assert_eq!(token, None);
        assert_eq!(state, None);
    }

    #[test]
    fn url_decodes_percent_escapes() {
        assert_eq!(url_decode("a%2Bb%20c"), "a+b c");
        assert_eq!(url_decode("plain-token_value"), "plain-token_value");
    }

    #[test]
    fn url_encode_escapes_query_values() {
        assert_eq!(url_encode("org_123"), "org_123");
        assert_eq!(url_encode("org with spaces"), "org%20with%20spaces");
    }

    #[test]
    fn login_target_appends_requested_org() {
        assert_eq!(
            login_target(
                "http://localhost:3000/connect/cli",
                9123,
                "state",
                Some("org_123")
            ),
            "http://localhost:3000/connect/cli?port=9123&state=state&org_id=org_123"
        );
        assert_eq!(
            login_target("http://localhost:3000/connect/cli", 9123, "state", None),
            "http://localhost:3000/connect/cli?port=9123&state=state"
        );
    }

    #[test]
    fn decodes_jwt_claims() {
        // header.payload.signature with payload {"org_id":"org_1","email":"a@b.c"}
        let payload = URL_SAFE_NO_PAD.encode(br#"{"org_id":"org_1","email":"a@b.c"}"#);
        let token = format!("h.{payload}.s");
        let claims = decode_claims(&token).expect("claims");
        assert_eq!(claims_str(&claims, "org_id").as_deref(), Some("org_1"));
        assert_eq!(claims_str(&claims, "email").as_deref(), Some("a@b.c"));
    }
}
