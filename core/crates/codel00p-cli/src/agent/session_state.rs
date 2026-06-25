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
    let mut metadata = SessionMetadata::new(session_state.session_id().clone(), source)
        .with_created_at(now_millis());
    let auto_title = crate::session::session_title_from_messages(session_state.messages());
    if let Some(title) = auto_title.clone() {
        metadata = metadata.with_title(title);
    }
    if let Some(parent) = parent {
        metadata = metadata.with_parent(parent);
    }
    match store.create_session(metadata) {
        Ok(()) => {}
        // The session already exists (a follow-up turn). Apply the auto-title only
        // if it still has none, so a user rename (set via `sessions rename` or the
        // TUI switcher) is never clobbered by re-deriving from the first message.
        Err(SessionStoreError::SessionAlreadyExists { .. }) => {
            if let Some(title) = auto_title {
                let has_title = store
                    .metadata(session_state.session_id())
                    .map(|existing| existing.title().is_some())
                    .unwrap_or(false);
                if !has_title {
                    store
                        .set_session_title(session_state.session_id(), &title)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
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

/// Current unix time in milliseconds, used to stamp a session's creation time so
/// `agent continue` can resume the most recent conversation. A clock error before
/// the epoch is treated as `0` rather than failing a persist.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// The id of the most recently created session: the highest `created_at`, with
/// undated (pre-timestamp) sessions treated as oldest and ties broken by id so
/// the choice is deterministic. `None` when there are no sessions.
pub(super) fn latest_session_id(sessions: &[SessionMetadata]) -> Option<String> {
    sessions
        .iter()
        .max_by(|left, right| {
            left.created_at()
                .unwrap_or(0)
                .cmp(&right.created_at().unwrap_or(0))
                .then_with(|| left.session_id().as_str().cmp(right.session_id().as_str()))
        })
        .map(|metadata| metadata.session_id().as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::{latest_session_id, persist_session_records};
    use crate::config::{CliConfig, open_session_store};
    use codel00p_harness::{SessionState, UserMessage};
    use codel00p_protocol::{ProjectRef, SessionId};
    use codel00p_session::{SessionMetadata, SessionStore};

    fn dated(id: &'static str, created_at: u64) -> SessionMetadata {
        SessionMetadata::new(SessionId::from_static(id), "cli").with_created_at(created_at)
    }

    fn test_config(dir: &std::path::Path) -> CliConfig {
        CliConfig {
            memory_db: dir.join("memory.sqlite"),
            organization_id: "org-1".to_string(),
            project: ProjectRef::new("project-1", "codel00p"),
            memory_ranking: Default::default(),
        }
    }

    #[test]
    fn picks_the_newest_dated_session() {
        let sessions = vec![
            dated("session-a", 100),
            dated("session-c", 300),
            dated("session-b", 200),
        ];
        assert_eq!(latest_session_id(&sessions).as_deref(), Some("session-c"));
    }

    #[test]
    fn undated_sessions_sort_oldest() {
        let sessions = vec![
            SessionMetadata::new(SessionId::from_static("session-legacy"), "cli"),
            dated("session-new", 1),
        ];
        assert_eq!(latest_session_id(&sessions).as_deref(), Some("session-new"));
    }

    #[test]
    fn ties_break_by_id_and_empty_is_none() {
        let sessions = vec![dated("session-b", 5), dated("session-a", 5)];
        assert_eq!(latest_session_id(&sessions).as_deref(), Some("session-b"));
        assert_eq!(latest_session_id(&[]), None);
    }

    #[test]
    fn persist_session_records_titles_new_sessions_from_first_user_message() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = test_config(dir.path());
        let mut session_state = SessionState::new(SessionId::from_static("chat-titled"));
        session_state.push_user(UserMessage::new("  Explain\n release   readiness  "));
        session_state.push_assistant("ready");

        persist_session_records(&config, &session_state, &[], 0, "cli", None).expect("persist");

        let store = open_session_store(&config).expect("open store");
        let metadata = store
            .metadata(session_state.session_id())
            .expect("metadata");
        assert_eq!(metadata.title(), Some("Explain release readiness"));
    }

    #[test]
    fn persist_session_records_keeps_a_user_rename_across_turns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = test_config(dir.path());
        let session_id = SessionId::from_static("chat-renamed");

        // First turn auto-titles the session from the first user message.
        let mut first = SessionState::new(session_id.clone());
        first.push_user(UserMessage::new("Original question"));
        first.push_assistant("answer one");
        persist_session_records(&config, &first, &[], 0, "cli", None).expect("persist first");

        // The user renames the session.
        {
            let mut store = open_session_store(&config).expect("open store");
            store
                .set_session_title(&session_id, "My pinned chat")
                .expect("rename");
        }

        // A follow-up turn must not re-derive (and clobber) the title.
        let mut second = SessionState::new(session_id.clone());
        second.push_user(UserMessage::new("Original question"));
        second.push_assistant("answer one");
        second.push_user(UserMessage::new("A follow-up"));
        second.push_assistant("answer two");
        persist_session_records(&config, &second, &[], 2, "cli", None).expect("persist second");

        let store = open_session_store(&config).expect("open store");
        let metadata = store.metadata(&session_id).expect("metadata");
        assert_eq!(metadata.title(), Some("My pinned chat"));
    }
}
