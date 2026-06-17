//! Structured assertions over a completed run.

use std::process::Output;

pub use codel00p_protocol::AgentEvent;

/// The structured outcome of one binary invocation.
pub struct RunResult {
    output: Output,
    stdout: String,
    stderr: String,
    events: Vec<AgentEvent>,
}

impl RunResult {
    pub(crate) fn new(output: Output) -> Self {
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        // `--json-events` prints one JSON event per line, mixed with any final
        // assistant text line. Parse every line that deserializes into an event.
        let events = stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<AgentEvent>(line).ok())
            .collect();
        Self {
            output,
            stdout,
            stderr,
            events,
        }
    }

    /// The process's raw stdout.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// The process's raw stderr.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Whether the process exited with a success status.
    #[must_use]
    pub fn success(&self) -> bool {
        self.output.status.success()
    }

    /// Asserts the process succeeded, panicking with stderr context otherwise.
    pub fn assert_success(&self) -> &Self {
        assert!(
            self.success(),
            "command failed (status {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.output.status.code(),
            self.stdout,
            self.stderr
        );
        self
    }

    /// All parsed `--json-events` events, in emission order.
    #[must_use]
    pub fn events(&self) -> &[AgentEvent] {
        &self.events
    }

    /// Returns `true` if any event satisfies `predicate`.
    #[must_use]
    pub fn has_event(&self, predicate: impl Fn(&AgentEvent) -> bool) -> bool {
        self.events.iter().any(predicate)
    }

    /// Asserts at least one event satisfies `predicate`.
    pub fn assert_event(&self, predicate: impl Fn(&AgentEvent) -> bool) -> &Self {
        assert!(
            self.has_event(predicate),
            "no event matched the predicate.\nevents: {:#?}",
            self.events
        );
        self
    }

    /// Asserts a `ToolCallRequested` was emitted for `name`.
    pub fn assert_tool_requested(&self, name: &str) -> &Self {
        self.assert_event(|event| {
            matches!(
                event,
                AgentEvent::ToolCallRequested { tool_name, .. } if tool_name == name
            )
        })
    }

    /// Asserts a `ToolCallCompleted` was emitted for `name`.
    pub fn assert_tool_completed(&self, name: &str) -> &Self {
        self.assert_event(|event| {
            matches!(
                event,
                AgentEvent::ToolCallCompleted { tool_name, .. } if tool_name == name
            )
        })
    }

    /// Asserts both a request and a completion were emitted for `name`.
    pub fn assert_tool_called(&self, name: &str) -> &Self {
        self.assert_tool_requested(name);
        self.assert_tool_completed(name)
    }

    /// Asserts a `TurnCompleted` event was emitted.
    pub fn assert_turn_completed(&self) -> &Self {
        self.assert_event(|event| matches!(event, AgentEvent::TurnCompleted { .. }))
    }

    /// Returns the first `ContextManifest` event, if any.
    #[must_use]
    pub fn context_manifest(&self) -> Option<&AgentEvent> {
        self.events
            .iter()
            .find(|event| matches!(event, AgentEvent::ContextManifest { .. }))
    }

    /// Asserts a `ContextManifest` was emitted and returns it.
    pub fn assert_context_manifest(&self) -> &AgentEvent {
        self.context_manifest()
            .unwrap_or_else(|| panic!("no ContextManifest event.\nevents: {:#?}", self.events))
    }
}
