//! Messaging gateway core: one agent core, reachable per conversation.
//!
//! A platform adapter (Slack, Telegram, an HTTP webhook, the `gateway message`
//! CLI) turns an inbound chat message into a [`GatewayCommand`] and maps the
//! conversation to a durable agent session via [`conversation_session_id`], so a
//! chat thread is a continuous agent session. Control commands (`/help`,
//! `/stop`, `/approve`, `/deny`) are handled before the agent runs.
//!
//! This crate is the platform-agnostic core (the first slice of the
//! [Messaging Gateway initiative](../../../docs/initiatives/messaging-gateway.md));
//! concrete network adapters build on it in later slices.

/// What an inbound message resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayCommand {
    /// Show what the gateway can do.
    Help,
    /// Stop the running turn (control command, bypasses the agent).
    Stop,
    /// Approve a pending permission request.
    Approve,
    /// Deny a pending permission request.
    Deny,
    /// Ordinary text to run as an agent turn.
    Message(String),
}

/// Parse an inbound message into a [`GatewayCommand`]. A leading `/word` is a
/// control command; anything else is a message to run.
pub fn parse_command(text: &str) -> GatewayCommand {
    let trimmed = text.trim();
    match trimmed
        .strip_prefix('/')
        .map(|rest| {
            rest.split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
        })
        .as_deref()
    {
        Some("help") => GatewayCommand::Help,
        Some("stop") | Some("cancel") => GatewayCommand::Stop,
        Some("approve") | Some("yes") | Some("allow") => GatewayCommand::Approve,
        Some("deny") | Some("no") | Some("reject") => GatewayCommand::Deny,
        _ => GatewayCommand::Message(trimmed.to_string()),
    }
}

/// The help shown for `/help`.
pub fn help_text() -> &'static str {
    "I'm your codel00p agent. Send a message and I'll work on it in this \
     conversation (I remember the thread). Controls: /help, /stop, /approve, /deny."
}

/// Derive a stable, id-safe session id for a conversation, so every message in
/// the same conversation continues one agent session.
pub fn conversation_session_id(conversation: &str) -> String {
    let mut out = String::from("gateway-");
    // The prefix already ends in a dash, so suppress a leading separator.
    let mut prev_dash = true;
    for ch in conversation.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.len() > "gateway-".len() {
        trimmed.to_string()
    } else {
        // Empty/symbol-only conversation id: keep a stable fallback.
        "gateway-default".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_control_commands_and_messages() {
        assert_eq!(parse_command("/help"), GatewayCommand::Help);
        assert_eq!(parse_command("  /Stop now "), GatewayCommand::Stop);
        assert_eq!(parse_command("/approve"), GatewayCommand::Approve);
        assert_eq!(parse_command("/deny"), GatewayCommand::Deny);
        assert_eq!(
            parse_command("fix the build"),
            GatewayCommand::Message("fix the build".to_string())
        );
        // An unknown slash command is treated as a message, not silently dropped.
        assert_eq!(
            parse_command("/unknown thing"),
            GatewayCommand::Message("/unknown thing".to_string())
        );
    }

    #[test]
    fn conversation_ids_are_stable_and_safe() {
        assert_eq!(conversation_session_id("C123"), "gateway-c123");
        assert_eq!(
            conversation_session_id("slack/#general"),
            "gateway-slack-general"
        );
        assert_eq!(conversation_session_id("  ../escape  "), "gateway-escape");
        assert_eq!(conversation_session_id("!!!"), "gateway-default");
        // Same conversation always maps to the same session.
        assert_eq!(
            conversation_session_id("Team-Chat"),
            conversation_session_id("team-chat")
        );
    }
}
