//! `codel00p-e2e` — a black-box, fully hermetic end-to-end test harness.
//!
//! The harness drives the **real** `codel00p` binary as a subprocess against a
//! scripted mock provider (an [`httpmock`] server speaking the OpenAI
//! chat-completions wire format that the `custom` provider expects). No network
//! access, no API keys, and no shared global state: every run gets its own
//! tempdir `CODEL00P_HOME`, its own workspace directory, and its own
//! `memory.sqlite`.
//!
//! # Layout
//!
//! - [`CodelRunner`] — the isolated-world builder. Seeds a workspace, optionally
//!   `git init`s it, attaches a [`MockProvider`], and spawns the binary.
//! - [`MockProvider`] — a fluent builder for a scripted, ordered conversation
//!   (assistant text turns and `tool_calls` turns) returned in sequence as the
//!   binary hits `POST /chat/completions`.
//! - [`RunResult`] — the structured outcome: stdout/stderr/exit status plus the
//!   parsed `--json-events` NDJSON stream as real [`codel00p_protocol::AgentEvent`]s.
//!
//! # Binary resolution
//!
//! `assert_cmd::cargo_bin("codel00p")` does **not** work from this crate: the
//! `CARGO_BIN_EXE_codel00p` env var is only injected for the crate that owns the
//! binary target (`codel00p-cli`). We therefore resolve the binary from the
//! workspace target directory relative to `CARGO_MANIFEST_DIR`. The dev-dependency
//! on `codel00p-cli` guarantees Cargo builds the binary before these tests run.
//! See [`codel00p_binary`].

mod assertions;
mod mock;
mod runner;

pub use assertions::{AgentEvent, RunResult};
pub use mock::MockProvider;
pub use runner::{CodelRunner, codel00p_binary};
