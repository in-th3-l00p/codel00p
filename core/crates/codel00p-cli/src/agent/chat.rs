//! Interactive chat loop and slash-command helpers.

use super::*;

pub(super) fn run_agent_chat(config: CliConfig, mut options: AgentRunOptions) -> CliResult<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start async runtime: {error}"))?;

    runtime.block_on(async move {
        let mut mcp_servers = load_mcp_servers_from_workspace(&options.workspace)?;
        mcp_servers.extend(options.mcp_servers.clone());

        // A bare `codel00p` chat starts a fresh conversation each launch. Resuming
        // is explicit (`--session-id`, or `/sessions` to find one) — never the
        // implicit default, which `SessionId::default()` would collapse onto a
        // single process-counter id (`session-1`) shared across every launch,
        // replaying an unbounded history until it overflows the context window.
        let session_id = match options.session_id.as_deref() {
            Some(value) => parse_session_id(value)?,
            None => parse_session_id(&fresh_chat_session_id())?,
        };
        let (mut session_state, mut persisted_message_count) =
            load_chat_session_state(&config, session_id)?;

        let mut stderr = io::stderr();
        writeln!(
            stderr,
            "codel00p chat — provider {} model {} (session {})",
            options.provider,
            options.model,
            session_state.session_id().as_str()
        )
        .ok();
        if persisted_message_count > 0 {
            writeln!(
                stderr,
                "Resumed conversation with {persisted_message_count} prior message(s)."
            )
            .ok();
        }
        writeln!(
            stderr,
            "Type a message and press Enter. Use /help for commands, /exit to quit."
        )
        .ok();

        loop {
            write!(stderr, "\nyou> ").ok();
            stderr.flush().ok();

            let mut line = String::new();
            let bytes = io::stdin()
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            if bytes == 0 {
                writeln!(stderr, "\nGoodbye.").ok();
                break;
            }

            let prompt = line.trim();
            if prompt.is_empty() {
                continue;
            }

            if let Some(command) = prompt.strip_prefix('/') {
                let (name, argument) = split_chat_command(command);
                match name {
                    "sessions" => {
                        write!(stderr, "{}", chat_sessions_listing(&config)?).ok();
                        continue;
                    }
                    "history" => {
                        write!(stderr, "{}", chat_history_listing(&session_state)).ok();
                        continue;
                    }
                    "tools" => {
                        let registry =
                            build_tool_registry(&options.tool_sets, &mcp_servers).await?;
                        write!(stderr, "{}", chat_tools_listing(&registry)).ok();
                        continue;
                    }
                    "model" => {
                        match argument {
                            Some(model) => {
                                options.model = model.to_string();
                                writeln!(stderr, "Model set to {}.", options.model).ok();
                            }
                            None => {
                                writeln!(
                                    stderr,
                                    "model {} (provider {})",
                                    options.model, options.provider
                                )
                                .ok();
                            }
                        }
                        continue;
                    }
                    "memory" => {
                        write!(stderr, "{}", chat_memory_listing(&config)?).ok();
                        continue;
                    }
                    _ => match handle_chat_command(
                        name,
                        &mut session_state,
                        &mut persisted_message_count,
                        &mut stderr,
                    ) {
                        ChatControl::Continue => continue,
                        ChatControl::Exit => break,
                    },
                }
            }

            let harness =
                build_agent_harness(&config, &options, &mcp_servers, session_state.session_id())
                    .await?;
            let outcome = match harness
                .run_turn_with_state(session_state.clone(), UserMessage::new(prompt.to_string()))
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    // A failed turn (e.g. an unsupported model) explains itself and
                    // keeps the chat open — `session_state` is unchanged (we passed
                    // a clone), so the user can `/model <id>` and retry instead of
                    // losing the whole conversation.
                    writeln!(
                        stderr,
                        "\n{}",
                        crate::error_help::humanize_provider_error(
                            &error.to_string(),
                            &options.provider,
                            &options.model,
                        )
                    )
                    .ok();
                    continue;
                }
            };

            if let Some(message) = &outcome.assistant_message {
                if options.stream {
                    // Tokens already streamed to stdout; end the line.
                    println!();
                } else {
                    println!("{message}");
                }
            } else {
                writeln!(stderr, "(no assistant response)").ok();
            }
            if options.json_events {
                for event in &outcome.events {
                    println!(
                        "{}",
                        serde_json::to_string(&event).map_err(|error| error.to_string())?
                    );
                }
            }

            persist_turn_outcome(
                &config,
                &outcome.session_state,
                &outcome.events,
                persisted_message_count,
            )?;
            persisted_message_count = outcome.session_state.messages().len();
            session_state = outcome.session_state;
        }

        Ok(String::new())
    })
}

enum ChatControl {
    Continue,
    Exit,
}

fn handle_chat_command(
    command: &str,
    session_state: &mut codel00p_harness::SessionState,
    persisted_message_count: &mut usize,
    stderr: &mut io::Stderr,
) -> ChatControl {
    let name = command.trim();
    match name {
        "exit" | "quit" | "q" => {
            writeln!(stderr, "Goodbye.").ok();
            ChatControl::Exit
        }
        "help" | "?" => {
            writeln!(
                stderr,
                "Commands:\n  \
                 /help              Show this help\n  \
                 /session           Show the current session id\n  \
                 /sessions          List all persisted conversations\n  \
                 /history           Show the current conversation\n  \
                 /tools             List the tools available this turn\n  \
                 /model [id]        Show or switch the model for later turns\n  \
                 /memory            Show approved project memory in context\n  \
                 /reset             Start a new conversation\n  \
                 /exit, /quit       Leave the chat"
            )
            .ok();
            ChatControl::Continue
        }
        "session" => {
            writeln!(stderr, "session {}", session_state.session_id().as_str()).ok();
            ChatControl::Continue
        }
        "reset" | "clear" => {
            *session_state =
                codel00p_harness::SessionState::new(codel00p_harness::SessionId::default());
            *persisted_message_count = 0;
            writeln!(
                stderr,
                "Started a new conversation (session {}).",
                session_state.session_id().as_str()
            )
            .ok();
            ChatControl::Continue
        }
        other => {
            writeln!(stderr, "Unknown command: /{other}. Try /help.").ok();
            ChatControl::Continue
        }
    }
}

fn split_chat_command(command: &str) -> (&str, Option<&str>) {
    let command = command.trim();
    match command.split_once(char::is_whitespace) {
        Some((name, rest)) => {
            let rest = rest.trim();
            (name, if rest.is_empty() { None } else { Some(rest) })
        }
        None => (command, None),
    }
}

/// A unique session id for a freshly launched chat, so each launch is its own
/// conversation rather than colliding on the process-counter default.
pub(crate) fn fresh_chat_session_id() -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("chat-{stamp}")
}

pub(crate) fn load_chat_session_state(
    config: &CliConfig,
    session_id: codel00p_harness::SessionId,
) -> CliResult<(codel00p_harness::SessionState, usize)> {
    let store = open_session_store(config)?;
    match store.metadata(&session_id) {
        Ok(_) => {
            let session_state = replay_session_messages(config, session_id)?;
            let count = session_state.messages().len();
            Ok((session_state, count))
        }
        Err(SessionStoreError::SessionNotFound { .. }) => {
            Ok((codel00p_harness::SessionState::new(session_id), 0))
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn chat_sessions_listing(config: &CliConfig) -> CliResult<String> {
    let store = open_session_store(config)?;
    let mut sessions = store.list_sessions().map_err(|error| error.to_string())?;
    if sessions.is_empty() {
        return Ok("No saved conversations yet.\n".to_string());
    }

    // Most recent first, matching `session list`; undated sessions sink last.
    sessions.sort_by(|left, right| {
        right
            .created_at()
            .cmp(&left.created_at())
            .then_with(|| left.session_id().as_str().cmp(right.session_id().as_str()))
    });

    let mut output = String::new();
    for metadata in sessions {
        let records = store
            .replay(metadata.session_id())
            .map_err(|error| error.to_string())?;
        let messages = records
            .iter()
            .filter(|record| matches!(record.record(), SessionRecord::Message(_)))
            .count();
        let title = metadata.title().map(str::to_string).or_else(|| {
            crate::session::session_title_from_messages(records.iter().filter_map(|record| {
                match record.record() {
                    SessionRecord::Message(message) => Some(message),
                    SessionRecord::Event(_) => None,
                }
            }))
        });
        output.push_str(&format!(
            "  {}\t{}\t{}\t{} message(s)\n",
            metadata.session_id().as_str(),
            title.as_deref().unwrap_or("Untitled conversation"),
            metadata.source(),
            messages
        ));
    }
    Ok(output)
}

/// A prior conversation summarized for the TUI session switcher: its id, the source
/// that created it (`cli`, `gateway`, …), and the number of messages in it. Mirrors
/// [`chat_sessions_listing`] but returns structured rows instead of a text blob.
pub(crate) struct ChatSessionSummary {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) source: String,
    pub(crate) message_count: usize,
}

/// Lists prior conversations, most-recent first, for the TUI session switcher.
pub(crate) fn chat_session_summaries(config: &CliConfig) -> CliResult<Vec<ChatSessionSummary>> {
    let store = open_session_store(config)?;
    let mut sessions = store.list_sessions().map_err(|error| error.to_string())?;
    // Newest first: undated sessions sort oldest, ties broken by id (see
    // `latest_session_id`), so the ordering is deterministic.
    sessions.sort_by(|left, right| {
        right
            .created_at()
            .unwrap_or(0)
            .cmp(&left.created_at().unwrap_or(0))
            .then_with(|| left.session_id().as_str().cmp(right.session_id().as_str()))
    });

    let mut summaries = Vec::with_capacity(sessions.len());
    for metadata in sessions {
        let records = store
            .replay(metadata.session_id())
            .map_err(|error| error.to_string())?;
        let message_count = records
            .iter()
            .filter(|record| matches!(record.record(), SessionRecord::Message(_)))
            .count();
        let title = metadata.title().map(str::to_string).or_else(|| {
            crate::session::session_title_from_messages(records.iter().filter_map(|record| {
                match record.record() {
                    SessionRecord::Message(message) => Some(message),
                    SessionRecord::Event(_) => None,
                }
            }))
        });
        summaries.push(ChatSessionSummary {
            session_id: metadata.session_id().as_str().to_string(),
            title,
            source: metadata.source().to_string(),
            message_count,
        });
    }
    Ok(summaries)
}

pub(crate) fn chat_history_listing(session_state: &codel00p_harness::SessionState) -> String {
    let messages = session_state.messages();
    if messages.is_empty() {
        return "No messages in this conversation yet.\n".to_string();
    }

    let mut output = String::new();
    for message in messages {
        let role = session_role_label(message.role());
        let summary = session_message_summary(message);
        output.push_str(&format!("  {role}: {summary}\n"));
    }
    output
}

pub(super) fn chat_tools_listing(registry: &ToolRegistry) -> String {
    let names = registry.names();
    if names.is_empty() {
        return "No tools enabled. Use --tool-set to enable some.\n".to_string();
    }

    let mut names = names;
    names.sort();
    let mut output = String::new();
    for name in names {
        output.push_str(&format!("  {name}\n"));
    }
    output
}

pub(crate) fn chat_memory_listing(config: &CliConfig) -> CliResult<String> {
    let store = open_memory_store(config)?;
    let items = store
        .retrieve(MemoryQuery::new(config.project.clone()).with_limit(10))
        .map_err(|error| error.to_string())?;
    if items.is_empty() {
        return Ok("No approved project memory yet.\n".to_string());
    }

    let mut output = String::new();
    for memory in items {
        let entry = memory.entry();
        output.push_str(&format!(
            "  {}\t{:?}\t{}\n",
            entry.id(),
            entry.kind(),
            entry.content()
        ));
    }
    Ok(output)
}
