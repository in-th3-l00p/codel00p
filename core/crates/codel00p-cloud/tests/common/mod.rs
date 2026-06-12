//! Shared scaffolding for the HTTP e2e tests: a local RSA key that stands in for
//! Clerk's signing key, token minting, and a server spawner.
//!
//! Each test binary compiles this module independently, so not every helper is
//! used by every binary.
#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use codel00p_cloud::{AppState, JwtVerifier, app};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use tokio::net::TcpListener;

pub const TEST_KID: &str = "test-key";

pub const PRIVATE_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC9Ol3D853++/TE
+3BoGGRPc0EQB8i91Eahw5deKfc7aEhzZZ3IDpseWjuHACB3jkS5U6R1LM8QbXjU
CSWHHco3wfx6oRnZqgEZm3oXhHQeI7gtfkxuPh0aquZRkKNcRuOXcDJ5qbnQjgFw
wCdVPVTlQhTjbqco/T0yGyKSuKfhSvpa5MrWlZvQ1+JTg1NNrElSo53V8jABMJUt
LLil+4K02T07+drlGnfgdH8Qg6u1amHTGH4dKgFNiLdOJY4HD0bxylX0VyP1f1lr
LFPBkUuLDhTk5aEa+73wJoWoek/iNiE7HUDdrm7ncpU5HLEAsGXMDwcqS5/hfMAb
F3itTVqzAgMBAAECggEAGuYFFim3N9vQ+39ShzmQaMrVYNX6byGRuMT462XDwyob
wmubdii9XB8vfw1BkD0k/8MoCZAJDyjAmEOEliRh7nMg1L250vsblOxI+rbVWsNx
FuZxLuqdcIECpG2PCzr4dzp3sluyEjdddQ2bib5iJwSxu3KrSGRXIpxA2eJt2tRy
dxvbSWR6281eCsjncXcELjbp+7SkUGnR1e0b+f1xJWP0ArEwyDEz4ftORQ/Nn59O
fgI3F7LDilnwn7FNxRvu0XXhY7ihlGRbOwMm798AYMyia14JU3rvIsswbpdgzWRm
JX2odsBSbVe5RN+AJHEXbQhx9rFbw+PltN/Vvmb3YQKBgQD9fupVG553hj23wdgt
+2oi0oGZK9fOZaj4YvXukPxv9sqvg43PLw0Rzb6dwmppTGjJkIQRBafrkm3bxdUg
m8AxL1p7ji9+z63QJgzzspGx5aeFXUWD2/D7i+UWEBlCKceFTOUYRcfgrTi8HzMl
B8GidZCy0K91svM6hxH3c5JOpQKBgQC/GOtV0dW4O93vbcEReI1mOClEdIAqSiGo
AjEFdnR9ZTZqXSghb96ua/yM/M/eut6xSQuoSQfEQCzpjqLvmy/Cn3azHTkyQT6y
ESzzmJAi1Gr6lxyF2M+NJMTPSQXyqMpnVm1qUDZS74hCDpHN0KsqtnSvKpVkZ3v1
O+86tNHcdwKBgEZCCsiT4xPVjP2FKFl2OTB1j53YXPPDkVVmeCsq3AxcJkkG+SLX
M5QfphkrbTrKBrD28OOW4beU2gXziuKCyH3ZVgawndFT1iS+pxBUCbV4pTl9ZGrr
ZpsRZuj6hUWlNrtnWIelr4RB/luFejNlNvHEC9rDpB3G/0rVbNFcosxRAoGARPjT
h8gSoUpKUi6E7q9aKbi/fEuoLptPBnq0Asq8RL4RI9a3s0nTT5T+NEzTIgrEcaxx
nq2tNfILw8iNmnmihVZU21UC3daasF5uoQVBkLCmZAfCbbTRRJouxroOgYTWePHC
0ApfcROvVFg529Ui0mnEN6zg+ro3DU4yjDfTPwUCgYEAhoCgw82eL+6y8sE9UW9F
vrEMX4oXZ4zBaGbM0umWU70mW9cMc5G1hwDOMeITb1iNZPRkTgkFgEKzcpoTFTWz
uBId4rEgBuwEo/A9x7gI6voo/+1fMSuLbvuYqkNAgCoLqvpE60uwsouByWilx+TA
lwnMVDi/V4DSTS3ZxOLjkF8=
-----END PRIVATE KEY-----";

pub const PUBLIC_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAvTpdw/Od/vv0xPtwaBhk
T3NBEAfIvdRGocOXXin3O2hIc2WdyA6bHlo7hwAgd45EuVOkdSzPEG141Aklhx3K
N8H8eqEZ2aoBGZt6F4R0HiO4LX5Mbj4dGqrmUZCjXEbjl3Ayeam50I4BcMAnVT1U
5UIU426nKP09Mhsikrin4Ur6WuTK1pWb0NfiU4NTTaxJUqOd1fIwATCVLSy4pfuC
tNk9O/na5Rp34HR/EIOrtWph0xh+HSoBTYi3TiWOBw9G8cpV9Fcj9X9ZayxTwZFL
iw4U5OWhGvu98CaFqHpP4jYhOx1A3a5u53KVORyxALBlzA8HKkuf4XzAGxd4rU1a
swIDAQAB
-----END PUBLIC KEY-----";

#[derive(Serialize)]
pub struct TestClaims {
    pub sub: String,
    pub exp: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_role: Option<String>,
}

pub fn future_exp() -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as usize;
    now + 3600
}

pub fn sign(claims: &TestClaims) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(TEST_KID.to_string());
    let key = EncodingKey::from_rsa_pem(PRIVATE_PEM).expect("encoding key");
    encode(&header, claims, &key).expect("sign token")
}

/// An admin of `org_acme` (or another org when `org_id` is given).
pub fn admin_token(org_id: &str) -> String {
    sign(&TestClaims {
        sub: "user_admin".into(),
        exp: future_exp(),
        email: Some("admin@team.dev".into()),
        org_id: Some(org_id.into()),
        org_slug: Some("acme".into()),
        org_name: Some("Acme Engineering".into()),
        org_role: Some("org:admin".into()),
    })
}

pub fn member_token(org_id: &str) -> String {
    sign(&TestClaims {
        sub: "user_member".into(),
        exp: future_exp(),
        email: None,
        org_id: Some(org_id.into()),
        org_slug: Some("acme".into()),
        org_name: Some("Acme Engineering".into()),
        org_role: Some("org:member".into()),
    })
}

pub fn test_verifier() -> JwtVerifier {
    JwtVerifier::from_rsa_pem(TEST_KID, PUBLIC_PEM, None).expect("verifier")
}

/// Boots the service with the given state on an ephemeral port; returns its URL.
pub async fn spawn(state: AppState) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app(state)).await.expect("serve");
    });
    format!("http://{addr}")
}
