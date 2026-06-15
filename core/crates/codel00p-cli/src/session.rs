use codel00p_protocol::{AgentEvent, SessionMessage, SessionRole};
use codel00p_session::{SessionRecord, SessionStore};
use serde_json::json;

use crate::config::{CliConfig, CliResult, open_session_store, parse_session_id, single_id};

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing session command".to_string());
    };

    match command.as_str() {
        "list" => session_list(config, rest),
        "show" => session_show(config, rest),
        _ => Err(format!("unknown session command: {command}")),
    }
}

struct SessionSummary {
    session_id: String,
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
        let event_count = records.len() - message_count;
        summaries.push(SessionSummary {
            session_id: metadata.session_id().as_str().to_string(),
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
            "{}\t{}\t{} message(s)\t{} event(s)\n",
            summary.session_id, summary.source, summary.message_count, summary.event_count
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

fn agent_event_label(event: &AgentEvent) -> &'static str {
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
        AgentEvent::TurnCompleted { .. } => "turn_completed",
    }
}
