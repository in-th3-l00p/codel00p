use codel00p_protocol::{AgentEvent, SessionMessage, SessionRole};
use codel00p_session::{SessionRecord, SessionStore};

use crate::config::{CliConfig, CliResult, open_session_store, parse_session_id, single_id};

pub fn run(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing session command".to_string());
    };

    match command.as_str() {
        "show" => session_show(config, rest),
        _ => Err(format!("unknown session command: {command}")),
    }
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

fn session_role_label(role: SessionRole) -> &'static str {
    match role {
        SessionRole::System => "system",
        SessionRole::User => "user",
        SessionRole::Assistant => "assistant",
        SessionRole::Tool => "tool",
    }
}

fn session_message_summary(message: &SessionMessage) -> String {
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
