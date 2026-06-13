//! Helpers for selecting turn context and summarizing compacted history.

use super::*;

/// The most recent user message text, used to rank skills for the turn.
pub(super) fn latest_user_message(state: &SessionState) -> String {
    state
        .messages()
        .iter()
        .rev()
        .find(|message| message.role() == SessionRole::User)
        .map(|message| message.content().to_string())
        .unwrap_or_default()
}

pub(super) fn summarize_compacted_messages(
    messages: &[SessionMessage],
    keep_recent: usize,
) -> String {
    let compacted_count = messages.len().saturating_sub(keep_recent);
    let mut lines = vec![format!(
        "Compacted {compacted_count} older session messages."
    )];
    for (index, message) in messages.iter().take(compacted_count).enumerate() {
        let mut content = message.content().replace('\n', " ");
        if content.len() > 200 {
            content.truncate(200);
            content.push_str("...");
        }
        lines.push(format!("{}. {:?}: {}", index + 1, message.role(), content));
    }
    lines.join("\n")
}
