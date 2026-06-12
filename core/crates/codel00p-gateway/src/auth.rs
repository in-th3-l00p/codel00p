//! Webhook authentication for the HTTP gateway.
//!
//! A gateway exposed over HTTP authenticates inbound requests with a single
//! shared secret presented as an HTTP `Authorization: Bearer <secret>` header.
//! There is no asymmetric crypto here: the secret is a pre-shared token, and the
//! comparison is done in (approximately) constant time so a network attacker
//! cannot learn the secret byte-by-byte from response timing.
//!
//! The integrator reads the configured secret (e.g. from the
//! `CODEL00P_GATEWAY_SECRET` environment variable or a config key) and calls
//! [`authorize`] on every protected request, returning `401` when it returns
//! `false`.

/// Extract the bearer token from an `Authorization` header value.
///
/// Returns the substring after the `"Bearer "` prefix, or `None` if the header
/// does not use the bearer scheme. The scheme name is matched case-sensitively
/// to keep the function simple and predictable; callers that need to accept
/// `bearer`/`BEARER` should normalize before calling.
///
/// ```
/// use codel00p_gateway::auth::bearer;
/// assert_eq!(bearer("Bearer s3cret"), Some("s3cret"));
/// assert_eq!(bearer("Basic abc"), None);
/// ```
pub fn bearer(header: &str) -> Option<&str> {
    header.strip_prefix("Bearer ")
}

/// Compare two byte slices in constant time with respect to their *contents*.
///
/// The running time depends only on the (already-public) length of the inputs,
/// not on where the first differing byte is. Slices of different lengths are
/// rejected immediately; for a fixed secret length this does not leak the
/// secret's contents. Implemented as a sum-of-xor accumulation so the loop
/// always visits every byte and never short-circuits.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Decide whether an inbound HTTP request is authorized.
///
/// - When no secret is configured (`configured_secret` is `None`), every request
///   is allowed. **This is insecure** and is intended only for trusted, local,
///   or loopback-only deployments where network access is already restricted; do
///   not expose such a gateway to untrusted networks.
/// - When a secret is configured, the request is allowed only if
///   `authorization_header` is exactly `"Bearer <secret>"`. The token is
///   compared to the configured secret in (content-)constant time via
///   [`constant_time_eq`]. A missing header, a non-bearer scheme, a
///   wrong-length token, or a wrong token are all rejected.
///
/// ```
/// use codel00p_gateway::auth::authorize;
/// // No secret configured: open.
/// assert!(authorize(None, None));
/// // Secret configured: must match.
/// assert!(authorize(Some("s3cret"), Some("Bearer s3cret")));
/// assert!(!authorize(Some("s3cret"), Some("Bearer nope")));
/// assert!(!authorize(Some("s3cret"), None));
/// ```
pub fn authorize(configured_secret: Option<&str>, authorization_header: Option<&str>) -> bool {
    let Some(secret) = configured_secret else {
        // No secret configured: trusted/local mode, allow everything.
        return true;
    };
    match authorization_header.and_then(bearer) {
        Some(token) => constant_time_eq(token.as_bytes(), secret.as_bytes()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_extracts_token() {
        assert_eq!(bearer("Bearer abc123"), Some("abc123"));
        assert_eq!(bearer("Bearer "), Some(""));
        assert_eq!(bearer("Basic abc123"), None);
        assert_eq!(bearer("bearer abc123"), None); // case-sensitive scheme
        assert_eq!(bearer("abc123"), None);
    }

    #[test]
    fn no_secret_allows_everything() {
        assert!(authorize(None, None));
        assert!(authorize(None, Some("Bearer whatever")));
        assert!(authorize(None, Some("garbage")));
    }

    #[test]
    fn correct_bearer_allows() {
        assert!(authorize(Some("s3cret"), Some("Bearer s3cret")));
        // Empty configured secret still requires an empty bearer token.
        assert!(authorize(Some(""), Some("Bearer ")));
    }

    #[test]
    fn wrong_or_missing_bearer_denies() {
        // Wrong token.
        assert!(!authorize(Some("s3cret"), Some("Bearer nope12")));
        // Missing header entirely.
        assert!(!authorize(Some("s3cret"), None));
        // Wrong scheme.
        assert!(!authorize(Some("s3cret"), Some("Basic s3cret")));
        // Token present but empty when a non-empty secret is configured.
        assert!(!authorize(Some("s3cret"), Some("Bearer ")));
    }

    #[test]
    fn wrong_length_denies() {
        // Shorter and longer than the configured secret are both rejected.
        assert!(!authorize(Some("s3cret"), Some("Bearer s3cre")));
        assert!(!authorize(Some("s3cret"), Some("Bearer s3crets")));
    }

    #[test]
    fn constant_time_eq_is_content_constant_time() {
        // Equal length, differing at the first byte vs the last byte must both
        // return false and (by construction) visit every byte.
        let secret = b"abcdefgh";
        let diff_first = b"zbcdefgh";
        let diff_last = b"abcdefgz";
        assert!(constant_time_eq(secret, secret));
        assert!(!constant_time_eq(secret, diff_first));
        assert!(!constant_time_eq(secret, diff_last));
        // Different lengths are rejected up front.
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }
}
