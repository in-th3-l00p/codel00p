use std::collections::HashMap;

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64_NO_PAD;
use codel00p_protocol::{OrgRef, OrgRole, Viewer};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

use crate::AppState;
use crate::error::ApiError;

/// The Clerk session-token claims codel00p reads. Clerk signs these with RS256;
/// organization claims are present only when the session has an active org.
///
/// Both Clerk token shapes are accepted: legacy flat `org_*` claims and the
/// newer compact `o` object (`{ id, slg, rol }`). Flat claims win when both are
/// present.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub org_slug: Option<String>,
    #[serde(default)]
    pub org_name: Option<String>,
    #[serde(default)]
    pub org_role: Option<String>,
    #[serde(default)]
    pub o: Option<CompactOrgClaim>,
}

/// Clerk's compact active-organization claim.
#[derive(Debug, Clone, Deserialize)]
pub struct CompactOrgClaim {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub slg: Option<String>,
    #[serde(default)]
    pub rol: Option<String>,
    #[serde(default)]
    pub nam: Option<String>,
}

impl Claims {
    fn resolved_org_id(&self) -> Option<String> {
        self.org_id
            .clone()
            .or_else(|| self.o.as_ref().and_then(|o| o.id.clone()))
    }

    fn resolved_org_slug(&self) -> Option<String> {
        self.org_slug
            .clone()
            .or_else(|| self.o.as_ref().and_then(|o| o.slg.clone()))
    }

    fn resolved_org_name(&self) -> Option<String> {
        self.org_name
            .clone()
            .or_else(|| self.o.as_ref().and_then(|o| o.nam.clone()))
    }

    fn resolved_org_role(&self) -> Option<String> {
        self.org_role
            .clone()
            .or_else(|| self.o.as_ref().and_then(|o| o.rol.clone()))
    }
}

/// Verifies RS256 JWTs against a set of public keys keyed by `kid`. Production
/// loads the keys from Clerk's JWKS endpoint; tests inject a local key. The key
/// source is the only thing that differs, which keeps the auth path fully
/// testable without a network or a live Clerk instance.
#[derive(Clone)]
pub struct JwtVerifier {
    keys: HashMap<String, DecodingKey>,
    issuer: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum VerifierError {
    #[error("jwks request failed: {0}")]
    Fetch(String),
    #[error("invalid jwks: {0}")]
    Jwks(String),
    #[error("invalid clerk publishable key")]
    PublishableKey,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

impl JwtVerifier {
    /// Builds a verifier from an explicit `kid -> DecodingKey` map. The issuer,
    /// when set, is checked on every token.
    pub fn new(keys: HashMap<String, DecodingKey>, issuer: Option<String>) -> Self {
        Self { keys, issuer }
    }

    /// Convenience constructor for a single PEM-encoded RSA public key — used by
    /// tests to mirror a single Clerk signing key.
    pub fn from_rsa_pem(
        kid: impl Into<String>,
        pem: &[u8],
        issuer: Option<String>,
    ) -> Result<Self, VerifierError> {
        let key =
            DecodingKey::from_rsa_pem(pem).map_err(|err| VerifierError::Jwks(err.to_string()))?;
        let mut keys = HashMap::new();
        keys.insert(kid.into(), key);
        Ok(Self::new(keys, issuer))
    }

    /// Loads signing keys from a Clerk JWKS endpoint (e.g.
    /// `https://<frontend-api>/.well-known/jwks.json`).
    pub async fn from_clerk_jwks(
        jwks_url: &str,
        issuer: Option<String>,
    ) -> Result<Self, VerifierError> {
        let jwks: Jwks = reqwest::get(jwks_url)
            .await
            .map_err(|err| VerifierError::Fetch(err.to_string()))?
            .json()
            .await
            .map_err(|err| VerifierError::Jwks(err.to_string()))?;

        let mut keys = HashMap::new();
        for jwk in jwks.keys {
            let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                .map_err(|err| VerifierError::Jwks(err.to_string()))?;
            keys.insert(jwk.kid, key);
        }

        if keys.is_empty() {
            return Err(VerifierError::Jwks("no signing keys".to_string()));
        }

        Ok(Self::new(keys, issuer))
    }

    /// Verifies a bearer token and returns its claims, or an `ApiError` that
    /// renders as `401`.
    pub fn verify(&self, token: &str) -> Result<Claims, ApiError> {
        let header =
            decode_header(token).map_err(|_| ApiError::Unauthorized("malformed token".into()))?;
        let kid = header
            .kid
            .ok_or_else(|| ApiError::Unauthorized("token missing key id".into()))?;
        let key = self
            .keys
            .get(&kid)
            .ok_or_else(|| ApiError::Unauthorized("unknown signing key".into()))?;

        let mut validation = Validation::new(Algorithm::RS256);
        // Clerk session tokens carry no `aud` by default; the issuer, when
        // configured, is the binding check.
        validation.validate_aud = false;
        if let Some(issuer) = &self.issuer {
            validation.set_issuer(&[issuer]);
        }

        decode::<Claims>(token, key, &validation)
            .map(|data| data.claims)
            .map_err(|err| ApiError::Unauthorized(format!("invalid token: {err}")))
    }
}

/// Derives a Clerk frontend API host from a publishable key. The key encodes
/// `<frontend-api>$` as base64 after the `pk_test_` / `pk_live_` prefix.
pub fn clerk_frontend_api_from_publishable_key(key: &str) -> Result<String, VerifierError> {
    let encoded = key
        .strip_prefix("pk_test_")
        .or_else(|| key.strip_prefix("pk_live_"))
        .ok_or(VerifierError::PublishableKey)?;
    // Clerk encodes the host with unpadded base64; tolerate stray padding too.
    let decoded = BASE64_NO_PAD
        .decode(encoded.trim_end_matches('='))
        .map_err(|_| VerifierError::PublishableKey)?;
    let host = String::from_utf8(decoded).map_err(|_| VerifierError::PublishableKey)?;
    let host = host.trim_end_matches('$').trim();
    if host.is_empty() {
        return Err(VerifierError::PublishableKey);
    }
    Ok(host.to_string())
}

/// The authenticated caller, resolved from a verified Clerk session token.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub email: Option<String>,
    pub org: Option<OrgRef>,
    pub org_role: Option<OrgRole>,
}

impl AuthContext {
    fn from_claims(claims: Claims) -> Self {
        let org_slug = claims.resolved_org_slug();
        let org_name = claims.resolved_org_name();
        let org = claims.resolved_org_id().map(|id| {
            let name = org_name
                .clone()
                .or_else(|| org_slug.clone())
                .unwrap_or_else(|| id.clone());
            let mut org = OrgRef::new(id, name);
            if let Some(slug) = &org_slug {
                org = org.with_slug(slug.clone());
            }
            org
        });
        let org_role = claims
            .resolved_org_role()
            .as_deref()
            .and_then(OrgRole::from_clerk_claim);

        Self {
            user_id: claims.sub,
            email: claims.email,
            org,
            org_role,
        }
    }

    /// Requires an active organization, returning its reference and the caller's
    /// role. Renders as `403` when the session has no active org.
    pub fn require_org(&self) -> Result<(&OrgRef, OrgRole), ApiError> {
        match (&self.org, self.org_role) {
            (Some(org), Some(role)) => Ok((org, role)),
            _ => Err(ApiError::Forbidden(
                "no active organization in session".into(),
            )),
        }
    }

    /// Requires an active organization in which the caller is an admin.
    pub fn require_org_admin(&self) -> Result<&OrgRef, ApiError> {
        let (org, role) = self.require_org()?;
        if role.is_admin() {
            Ok(org)
        } else {
            Err(ApiError::Forbidden("requires organization admin".into()))
        }
    }

    /// Projects the context into the protocol `Viewer` shape for `GET /me`.
    pub fn to_viewer(&self) -> Viewer {
        let mut viewer = Viewer::new(self.user_id.clone());
        if let Some(email) = &self.email {
            viewer = viewer.with_email(email.clone());
        }
        if let (Some(org), Some(role)) = (&self.org, self.org_role) {
            viewer = viewer.with_org(org.clone(), role);
        }
        viewer
    }
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("missing authorization header".into()))?;
        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or_else(|| ApiError::Unauthorized("expected bearer token".into()))?;

        let claims = state.verifier().verify(token.trim())?;
        Ok(AuthContext::from_claims(claims))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_frontend_api_from_real_dev_publishable_key() {
        // The desktop app's dev key decodes to its Clerk frontend API host.
        let host = clerk_frontend_api_from_publishable_key(
            "pk_test_c21vb3RoLWxlbXVyLTQ4LmNsZXJrLmFjY291bnRzLmRldiQ",
        )
        .expect("decode publishable key");
        assert_eq!(host, "smooth-lemur-48.clerk.accounts.dev");
    }

    #[test]
    fn rejects_malformed_publishable_keys() {
        assert!(clerk_frontend_api_from_publishable_key("nope").is_err());
        assert!(clerk_frontend_api_from_publishable_key("pk_test_!!!!").is_err());
    }

    #[test]
    fn auth_context_maps_clerk_org_claims() {
        let claims = Claims {
            sub: "user_1".into(),
            exp: 0,
            email: Some("dev@team.dev".into()),
            org_id: Some("org_1".into()),
            org_slug: Some("acme".into()),
            org_name: Some("Acme".into()),
            org_role: Some("org:admin".into()),
            o: None,
        };
        let context = AuthContext::from_claims(claims);

        let (org, role) = context.require_org().expect("org present");
        assert_eq!(org.id(), "org_1");
        assert_eq!(org.name(), "Acme");
        assert_eq!(org.slug(), Some("acme"));
        assert!(role.is_admin());
        assert!(context.require_org_admin().is_ok());

        let viewer = context.to_viewer();
        assert_eq!(viewer.user_id(), "user_1");
        assert_eq!(viewer.email(), Some("dev@team.dev"));
    }

    #[test]
    fn member_is_not_admin_and_missing_org_is_forbidden() {
        let member = AuthContext::from_claims(Claims {
            sub: "user_2".into(),
            exp: 0,
            email: None,
            org_id: Some("org_1".into()),
            org_slug: None,
            org_name: None,
            org_role: Some("org:member".into()),
            o: None,
        });
        assert!(member.require_org().is_ok());
        assert!(matches!(
            member.require_org_admin(),
            Err(ApiError::Forbidden(_))
        ));

        let solo = AuthContext::from_claims(Claims {
            sub: "user_3".into(),
            exp: 0,
            email: None,
            org_id: None,
            org_slug: None,
            org_name: None,
            org_role: None,
            o: None,
        });
        assert!(matches!(solo.require_org(), Err(ApiError::Forbidden(_))));
        assert_eq!(solo.to_viewer().org(), None);
    }

    #[test]
    fn auth_context_reads_compact_org_claim() {
        // Newer Clerk tokens carry org context in a compact `o` object.
        let claims: Claims = serde_json::from_value(serde_json::json!({
            "sub": "user_4",
            "exp": 0,
            "o": { "id": "org_9", "slg": "globex", "rol": "admin", "nam": "Globex" }
        }))
        .expect("deserialize compact claims");
        let context = AuthContext::from_claims(claims);

        let (org, role) = context.require_org().expect("org present");
        assert_eq!(org.id(), "org_9");
        assert_eq!(org.name(), "Globex");
        assert_eq!(org.slug(), Some("globex"));
        assert!(role.is_admin());
    }
}
