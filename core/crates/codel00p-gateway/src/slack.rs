//! Slack inbound ingest: parse Slack's HTTP Events API JSON into a small,
//! platform-agnostic model the gateway can act on.
//!
//! Slack delivers two kinds of payloads to a registered request URL:
//!
//! * a one-time `url_verification` handshake during setup, which carries a
//!   `challenge` string the endpoint must echo back, and
//! * `event_callback` envelopes wrapping a concrete `event` (we care about
//!   `message` events).
//!
//! Everything here is a pure function over the raw request body. Network I/O,
//! signature verification, and replying live in the `gateway serve` adapter; see
//! the crate docs and the messaging-gateway initiative for how the pieces fit.

use serde::Deserialize;

/// The meaningful outcome of parsing a Slack event payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlackEvent {
    /// Slack's setup handshake: echo `challenge` back in the HTTP 200 body.
    UrlVerification {
        /// The opaque string Slack expects to receive verbatim in the response.
        challenge: String,
    },
    /// A human message that should become an agent turn.
    Message {
        /// The Slack channel/conversation id (e.g. `C0123`); maps to a session.
        conversation: String,
        /// The Slack user id that sent the message (e.g. `U0123`).
        user: String,
        /// The message text.
        text: String,
    },
    /// A payload we deliberately drop (bot messages, edits/joins/other
    /// subtypes, or event types we do not handle). The adapter should ACK with
    /// an empty HTTP 200 so Slack does not retry.
    Ignored,
}

/// Why a Slack payload could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlackError {
    /// The body was not valid JSON, or did not match the expected shape.
    InvalidJson(String),
}

impl std::fmt::Display for SlackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlackError::InvalidJson(msg) => write!(f, "invalid Slack event JSON: {msg}"),
        }
    }
}

impl std::error::Error for SlackError {}

/// Top-level Slack envelope. Only the fields we branch on are modelled; unknown
/// fields are ignored so future Slack additions do not break parsing.
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    challenge: Option<String>,
    event: Option<InnerEvent>,
}

/// The inner `event` object of an `event_callback` envelope.
#[derive(Deserialize)]
struct InnerEvent {
    #[serde(rename = "type")]
    kind: String,
    channel: Option<String>,
    user: Option<String>,
    text: Option<String>,
    /// Present when the message was posted by a bot; such events are ignored to
    /// avoid loops (including the gateway replying to itself).
    bot_id: Option<String>,
    /// Present for non-plain messages (edits, joins, file shares, etc.); these
    /// are ignored — only plain user messages become agent turns.
    subtype: Option<String>,
}

/// Parse a raw Slack Events API request body into a [`SlackEvent`].
///
/// * `{"type":"url_verification","challenge":"X"}` →
///   [`SlackEvent::UrlVerification`].
/// * An `event_callback` wrapping a plain `message` event →
///   [`SlackEvent::Message`].
/// * A `message` event carrying `bot_id` or any `subtype`, or any other event /
///   envelope type → [`SlackEvent::Ignored`].
///
/// Returns [`SlackError::InvalidJson`] when the body is not valid JSON in the
/// expected envelope shape.
pub fn parse_slack_event(body: &str) -> Result<SlackEvent, SlackError> {
    let envelope: Envelope =
        serde_json::from_str(body).map_err(|e| SlackError::InvalidJson(e.to_string()))?;

    match envelope.kind.as_str() {
        "url_verification" => match envelope.challenge {
            Some(challenge) => Ok(SlackEvent::UrlVerification { challenge }),
            // A url_verification with no challenge is malformed.
            None => Err(SlackError::InvalidJson(
                "url_verification missing challenge".to_string(),
            )),
        },
        "event_callback" => match envelope.event {
            Some(event) => Ok(classify_event(event)),
            None => Ok(SlackEvent::Ignored),
        },
        // Any other envelope type (e.g. app_rate_limited) is acknowledged but
        // not acted upon.
        _ => Ok(SlackEvent::Ignored),
    }
}

/// Decide what to do with the inner event of an `event_callback`.
fn classify_event(event: InnerEvent) -> SlackEvent {
    if event.kind != "message" {
        return SlackEvent::Ignored;
    }
    // Drop bot messages and message subtypes: they must not become agent turns.
    if event.bot_id.is_some() || event.subtype.is_some() {
        return SlackEvent::Ignored;
    }
    match (event.channel, event.user, event.text) {
        (Some(conversation), Some(user), Some(text)) => SlackEvent::Message {
            conversation,
            user,
            text,
        },
        // A message missing channel/user/text is not actionable.
        _ => SlackEvent::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_verification_returns_challenge() {
        let body = r#"{"type":"url_verification","token":"t","challenge":"abc123"}"#;
        assert_eq!(
            parse_slack_event(body).unwrap(),
            SlackEvent::UrlVerification {
                challenge: "abc123".to_string()
            }
        );
    }

    #[test]
    fn url_verification_without_challenge_is_error() {
        let body = r#"{"type":"url_verification"}"#;
        assert!(matches!(
            parse_slack_event(body),
            Err(SlackError::InvalidJson(_))
        ));
    }

    #[test]
    fn plain_message_is_extracted() {
        let body = r#"{
            "type":"event_callback",
            "event":{
                "type":"message",
                "channel":"C0123",
                "user":"U0456",
                "text":"fix the build",
                "ts":"1700000000.000100"
            }
        }"#;
        assert_eq!(
            parse_slack_event(body).unwrap(),
            SlackEvent::Message {
                conversation: "C0123".to_string(),
                user: "U0456".to_string(),
                text: "fix the build".to_string(),
            }
        );
    }

    #[test]
    fn bot_message_is_ignored() {
        let body = r#"{
            "type":"event_callback",
            "event":{
                "type":"message",
                "channel":"C0123",
                "user":"U0456",
                "text":"beep boop",
                "bot_id":"B0789"
            }
        }"#;
        assert_eq!(parse_slack_event(body).unwrap(), SlackEvent::Ignored);
    }

    #[test]
    fn subtype_message_is_ignored() {
        let body = r#"{
            "type":"event_callback",
            "event":{
                "type":"message",
                "subtype":"message_changed",
                "channel":"C0123",
                "user":"U0456",
                "text":"edited"
            }
        }"#;
        assert_eq!(parse_slack_event(body).unwrap(), SlackEvent::Ignored);
    }

    #[test]
    fn non_message_event_is_ignored() {
        let body = r#"{
            "type":"event_callback",
            "event":{"type":"reaction_added","user":"U1","reaction":"thumbsup"}
        }"#;
        assert_eq!(parse_slack_event(body).unwrap(), SlackEvent::Ignored);
    }

    #[test]
    fn unknown_envelope_type_is_ignored() {
        let body = r#"{"type":"app_rate_limited","minute_rate_limited":1}"#;
        assert_eq!(parse_slack_event(body).unwrap(), SlackEvent::Ignored);
    }

    #[test]
    fn message_missing_text_is_ignored() {
        let body = r#"{
            "type":"event_callback",
            "event":{"type":"message","channel":"C0123","user":"U0456"}
        }"#;
        assert_eq!(parse_slack_event(body).unwrap(), SlackEvent::Ignored);
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(matches!(
            parse_slack_event("not json at all"),
            Err(SlackError::InvalidJson(_))
        ));
        assert!(matches!(
            parse_slack_event("{"),
            Err(SlackError::InvalidJson(_))
        ));
    }

    #[test]
    fn error_displays_message() {
        let err = parse_slack_event("oops").unwrap_err();
        assert!(err.to_string().contains("invalid Slack event JSON"));
    }
}
