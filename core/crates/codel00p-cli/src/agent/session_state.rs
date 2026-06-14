//! Session replay and persistence helpers for CLI agent turns.

use super::*;

pub(super) fn prepare_session_state(
    config: &CliConfig,
    options: &AgentRunOptions,
    session_mode: AgentSessionMode,
) -> CliResult<(codel00p_harness::SessionState, usize)> {
    match session_mode {
        AgentSessionMode::Fresh => {
            let session_id = options
                .session_id
                .as_deref()
                .map(parse_session_id)
                .transpose()?
                .unwrap_or_default();
            Ok((codel00p_harness::SessionState::new(session_id), 0))
        }
        AgentSessionMode::Resume => {
            let session_id = options
                .session_id
                .as_deref()
                .ok_or_else(|| "missing resume session id".to_string())
                .and_then(parse_session_id)?;
            let session_state = replay_session_messages(config, session_id)?;
            let previous_message_count = session_state.messages().len();
            Ok((session_state, previous_message_count))
        }
    }
}

pub(super) fn replay_session_messages(
    config: &CliConfig,
    session_id: codel00p_harness::SessionId,
) -> CliResult<codel00p_harness::SessionState> {
    let store = open_session_store(config)?;
    let records = store
        .replay(&session_id)
        .map_err(|error| error.to_string())?;
    let mut session_state = codel00p_harness::SessionState::new(session_id);

    for record in records {
        if let SessionRecord::Message(message) = record.record() {
            session_state.push_message(message.clone());
        }
    }

    Ok(session_state)
}

pub(crate) fn persist_turn_outcome(
    config: &CliConfig,
    session_state: &codel00p_harness::SessionState,
    events: &[AgentEvent],
    message_start_index: usize,
) -> CliResult<()> {
    persist_session_records(
        config,
        session_state,
        events,
        message_start_index,
        "cli",
        None,
    )
}

/// Persist a session's new messages and events, creating it with `source` and an
/// optional `parent` for lineage (used for sub-agent child sessions).
pub(super) fn persist_session_records(
    config: &CliConfig,
    session_state: &codel00p_harness::SessionState,
    events: &[AgentEvent],
    message_start_index: usize,
    source: &str,
    parent: Option<SessionId>,
) -> CliResult<()> {
    let mut store = open_session_store(config)?;
    let mut metadata = SessionMetadata::new(session_state.session_id().clone(), source);
    if let Some(parent) = parent {
        metadata = metadata.with_parent(parent);
    }
    match store.create_session(metadata) {
        Ok(()) | Err(SessionStoreError::SessionAlreadyExists { .. }) => {}
        Err(error) => return Err(error.to_string()),
    }

    for message in &session_state.messages()[message_start_index..] {
        store
            .append_message(session_state.session_id(), message.clone())
            .map_err(|error| error.to_string())?;
    }
    for event in events {
        store
            .append_event(session_state.session_id(), event.clone())
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}
