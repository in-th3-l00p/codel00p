use codel00p_protocol::{AgentEvent, SessionMessage, SessionRole};
use codel00p_session::{SessionRecord, SessionStore};
use serde_json::json;

use crate::config::{CliConfig, CliResult, open_session_store, parse_session_id, single_id};
use crate::settings::AgentSettings;

const SESSION_TITLE_MAX_CHARS: usize = 64;

pub fn run(config: CliConfig, agent_defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        // Bare `codel00p sessions` on a terminal opens the browser dialog; pipes
        // and CI keep the scriptable behavior so output is never corrupted. The
        // browser's resume action launches chat, so it needs the agent defaults.
        use std::io::IsTerminal;
        return if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
            crate::sessions_ui::run(config, &agent_defaults)
        } else {
            Err("missing session command".to_string())
        };
    };

    match command.as_str() {
        "list" => session_list(config, rest),
        "show" => session_show(config, rest),
        _ => Err(format!("unknown session command: {command}")),
    }
}

struct SessionSummary {
    session_id: String,
    title: Option<String>,
    source: String,
    parent_session_id: Option<String>,
    message_count: usize,
    event_count: usize,
    created_at: Option<u64>,
}

fn session_list(config: CliConfig, args: &[String]) -> CliResult<String> {
    let json_output = parse_list_flags(args)?;
    let store = open_session_store(&config)?;
    let mut summaries = Vec::new();

    for metadata in store.list_sessions().map_err(|error| error.to_string())? {
        let records = store
            .replay(metadata.session_id())
            .map_err(|error| error.to_string())?;
        let message_count = records
            .iter()
            .filter(|record| matches!(record.record(), SessionRecord::Message(_)))
            .count();
        let title = metadata.title().map(str::to_string).or_else(|| {
            session_title_from_messages(records.iter().filter_map(|record| match record.record() {
                SessionRecord::Message(message) => Some(message),
                SessionRecord::Event(_) => None,
            }))
        });
        let event_count = records.len() - message_count;
        summaries.push(SessionSummary {
            session_id: metadata.session_id().as_str().to_string(),
            title,
            source: metadata.source().to_string(),
            parent_session_id: metadata
                .parent_session_id()
                .map(|id| id.as_str().to_string()),
            message_count,
            event_count,
            created_at: metadata.created_at(),
        });
    }

    // Most recent first: newest `created_at` on top, undated (pre-timestamp)
    // sessions last, ties broken by id so the order is stable.
    summaries.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });

    if json_output {
        let records = summaries
            .iter()
            .map(session_summary_json)
            .collect::<Vec<_>>();
        return serde_json::to_string(&records).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for summary in &summaries {
        output.push_str(&format!(
            "{}\t{}\t{}\t{} message(s)\t{} event(s)\n",
            summary.session_id,
            summary.title.as_deref().unwrap_or("Untitled conversation"),
            summary.source,
            summary.message_count,
            summary.event_count
        ));
    }
    Ok(output)
}

fn parse_list_flags(args: &[String]) -> CliResult<bool> {
    let mut json_output = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json_output = true,
            flag => return Err(format!("unknown session list option: {flag}")),
        }
    }
    Ok(json_output)
}

fn session_summary_json(summary: &SessionSummary) -> serde_json::Value {
    json!({
        "session_id": summary.session_id,
        "title": summary.title,
        "source": summary.source,
        "parent_session_id": summary.parent_session_id,
        "message_count": summary.message_count,
        "event_count": summary.event_count,
        "created_at": summary.created_at,
    })
}

fn session_show(config: CliConfig, args: &[String]) -> CliResult<String> {
    let id = single_id(args, "session show")?;
    let session_id = parse_session_id(id)?;
    let store = open_session_store(&config)?;
    let records = store
        .replay(&session_id)
        .map_err(|error| error.to_string())?;
    let mut output = String::new();

    for record in records {
        match record.record() {
            SessionRecord::Message(message) => output.push_str(&format!(
                "{}\tmessage\t{}\t{}\n",
                record.sequence(),
                session_role_label(message.role()),
                session_message_summary(message)
            )),
            SessionRecord::Event(event) => output.push_str(&format!(
                "{}\tevent\t{}\t\n",
                record.sequence(),
                agent_event_label(event)
            )),
        }
    }

    Ok(output)
}

pub(crate) fn session_role_label(role: SessionRole) -> &'static str {
    match role {
        SessionRole::System => "system",
        SessionRole::User => "user",
        SessionRole::Assistant => "assistant",
        SessionRole::Tool => "tool",
    }
}

pub(crate) fn session_message_summary(message: &SessionMessage) -> String {
    if !message.content().is_empty() {
        return message.content().to_string();
    }
    if !message.tool_calls().is_empty() {
        return format!("{} tool call(s)", message.tool_calls().len());
    }
    String::new()
}

pub(crate) fn session_title_from_messages<'a>(
    messages: impl IntoIterator<Item = &'a SessionMessage>,
) -> Option<String> {
    messages.into_iter().find_map(|message| {
        if message.role() == SessionRole::User {
            normalized_session_title(message.content())
        } else {
            None
        }
    })
}

fn normalized_session_title(content: &str) -> Option<String> {
    let title = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return None;
    }
    if title.chars().count() <= SESSION_TITLE_MAX_CHARS {
        return Some(title);
    }

    let mut truncated = String::new();
    for word in title.split_whitespace() {
        let separator = usize::from(!truncated.is_empty());
        let candidate_len = truncated.chars().count() + separator + word.chars().count() + 3;
        if candidate_len > SESSION_TITLE_MAX_CHARS {
            break;
        }
        if !truncated.is_empty() {
            truncated.push(' ');
        }
        truncated.push_str(word);
    }

    if truncated.is_empty() {
        truncated = title
            .chars()
            .take(SESSION_TITLE_MAX_CHARS.saturating_sub(3))
            .collect::<String>();
    }

    Some(format!("{}...", truncated.trim_end()))
}

pub(crate) fn agent_event_label(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::SessionStarted { .. } => "session_started",
        AgentEvent::TurnStarted { .. } => "turn_started",
        AgentEvent::ContextBuilt { .. } => "context_built",
        AgentEvent::ContextCompacted { .. } => "context_compacted",
        AgentEvent::InferenceRequested { .. } => "inference_requested",
        AgentEvent::InferenceCompleted { .. } => "inference_completed",
        AgentEvent::ToolCallRequested { .. } => "tool_call_requested",
        AgentEvent::ToolCallCompleted { .. } => "tool_call_completed",
        AgentEvent::ToolCallFailed { .. } => "tool_call_failed",
        AgentEvent::PermissionRequested { .. } => "permission_requested",
        AgentEvent::PermissionDenied { .. } => "permission_denied",
        AgentEvent::ToolProgress { .. } => "tool_progress",
        AgentEvent::LifecycleHookFailed { .. } => "lifecycle_hook_failed",
        AgentEvent::ContextManifest { .. } => "context_manifest",
        AgentEvent::TurnCompleted { .. } => "turn_completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_title_from_messages_uses_first_user_message() {
        let messages = [
            SessionMessage::system("context"),
            SessionMessage::user("  Explain\n the   release process?  "),
            SessionMessage::user("second user message"),
        ];

        assert_eq!(
            session_title_from_messages(messages.iter()).as_deref(),
            Some("Explain the release process?")
        );
    }

    #[test]
    fn session_title_from_messages_caps_long_titles() {
        let messages = [SessionMessage::user(
            "Explain exactly how the command-line conversation switcher should rebuild history",
        )];

        assert_eq!(
            session_title_from_messages(messages.iter()).as_deref(),
            Some("Explain exactly how the command-line conversation switcher...")
        );
    }
}
