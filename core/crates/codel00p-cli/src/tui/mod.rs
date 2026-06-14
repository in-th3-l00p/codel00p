//! Full-screen terminal UI for the interactive agent (`codel00p` / `codel00p agent
//! chat`). The plain line REPL in `agent.rs` remains the non-TTY / machine-readable
//! fallback; see `agent::agent_chat`.
//!
//! Structure (Elm-style): [`app::App`] is the model, [`update::update`] the only
//! place state changes, [`view::render`] the pure draw, and [`event_loop`] the thin
//! async driver that turns terminal input and channel messages into [`Msg`]s and
//! executes the [`Effect`]s an update returns. [`bridge`] connects the async harness
//! turn to the UI channel.

pub(crate) mod app;
pub(crate) mod bridge;
pub(crate) mod cloud_data;
pub(crate) mod conversation;
pub(crate) mod event_loop;
pub(crate) mod msg;
pub(crate) mod overlay;
pub(crate) mod picker;
pub(crate) mod theme;
pub(crate) mod update;
pub(crate) mod view;

pub(crate) use msg::Msg;

use crate::agent::AgentRunOptions;
use crate::config::{CliConfig, CliResult};

/// Entry point: runs the interactive agent in a full-screen TUI. Builds a
/// multi-threaded runtime, sets up the terminal, and drives the event loop,
/// restoring the terminal on every exit path.
pub(crate) fn run_agent_tui(config: CliConfig, options: AgentRunOptions) -> CliResult<String> {
    event_loop::run(config, options)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::app::App;
    use crate::agent::{AgentRunOptions, CliPermissionMode};
    use crate::config::CliConfig;
    use codel00p_harness::{SessionId, SessionState};
    use codel00p_protocol::ProjectRef;
    use std::path::PathBuf;

    /// A minimal `App` for pure update/view tests — no real config, cloud, or turn.
    pub(crate) fn test_app() -> App {
        let config = CliConfig {
            memory_db: PathBuf::from("/tmp/codel00p-test.db"),
            organization_id: "org_test".to_string(),
            project: ProjectRef::new("proj_test", "test"),
        };
        let options = AgentRunOptions {
            prompt: String::new(),
            workspace: PathBuf::from("."),
            provider: "anthropic".to_string(),
            model: "claude-opus-4-8".to_string(),
            provider_policy_preset: None,
            base_url: None,
            session_id: None,
            max_iterations: None,
            json_events: false,
            stream_events: false,
            stream: true,
            tool_sets: Vec::new(),
            permission_mode: CliPermissionMode::Ask,
            remember_permissions: false,
            mcp_servers: Vec::new(),
            gateway_approval: None,
        };
        App::new(
            config,
            options,
            Vec::new(),
            SessionState::new(SessionId::from_static("session-test")),
            0,
            false,
        )
    }
}
