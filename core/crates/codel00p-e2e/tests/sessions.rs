//! End-to-end tests for session persistence and resume.
//!
//! Covers:
//! - `agent run` creates a session → `session list --json` shows it.
//! - `session show <id>` replays user + assistant messages from that run.
//! - **Resume**: `agent run` then `agent resume <id> "<follow-up>"` appends to
//!   the same session; the second turn's request includes prior conversation
//!   messages, and `session show` reports 4 messages (2 user + 2 assistant).
//! - **Continue**: `agent continue "<prompt>"` resumes the most-recent session
//!   without naming its id; the session count does not grow.
//! - **Recency ordering**: two separate sessions appear most-recent-first in
//!   `session list --json`, with `created_at` timestamps reflecting creation order.
//!
//! # Sub-agent lineage
//! Sub-agent sessions have a `parent_session_id` set by the delegation path in
//! `codel00p-cli/src/agent/delegation.rs` and persist via `persist_session_records`.
//! Exercising this end-to-end requires a delegation tool call and a nested agent
//! invocation, which is complex to script against the mock. That scenario is
//! covered by `tests/pilot.rs`'s tool-loop assertions (it verifies the full
//! delegation harness). The `parent_session_id` field is verified structurally in
//! `session list --json` here (we assert it is `null` for top-level sessions) but
//! a dedicated delegation lineage test is deferred to a separate `delegation.rs`
//! suite.
//!
//! Fully hermetic: no network access, no API keys. Each scenario gets its own
//! `CODEL00P_HOME` tempdir via [`CodelRunner::new`].

use codel00p_e2e::{CodelRunner, MockProvider};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse `session list --json` output from a runner into a JSON array.
fn list_sessions_json(runner: &CodelRunner) -> Vec<Value> {
    let result = runner.run(&["session", "list", "--json"]);
    result.assert_success();
    let raw = result.stdout().trim();
    serde_json::from_str::<Value>(raw)
        .unwrap_or_else(|error| {
            panic!("session list --json must produce valid JSON; error: {error}\nraw:\n{raw}")
        })
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("session list --json must return a JSON array, got:\n{raw}"))
}

/// Extract `session_id` from a session JSON object.
fn session_id_of(session: &Value) -> &str {
    session
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("session object missing 'session_id': {session}"))
}

// ---------------------------------------------------------------------------
// Scenario 1: agent run creates a session; session list --json shows it
// ---------------------------------------------------------------------------

#[test]
fn agent_run_creates_session_visible_in_list() {
    let runner = CodelRunner::new();

    // One simple turn: the model replies immediately with text.
    let provider = MockProvider::start().assistant_text("Task complete.");
    let runner = runner.with_provider(&provider);

    let result = runner.run(&["agent", "run", "Say hello."]);
    result.assert_success();

    // `session list --json` should return exactly one session.
    let sessions = list_sessions_json(&runner);
    assert_eq!(
        sessions.len(),
        1,
        "expected exactly one session after a single agent run, got: {sessions:#?}"
    );

    let session = &sessions[0];

    // Required fields are present and non-empty.
    let id = session_id_of(session);
    assert!(!id.is_empty(), "session_id should be non-empty");

    // `created_at` is present and is a non-zero integer (persisted with now_millis()).
    let created_at = session
        .get("created_at")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("session object missing 'created_at': {session}"));
    assert!(created_at > 0, "created_at should be a non-zero timestamp");

    // Top-level sessions have no parent.
    assert_eq!(
        session.get("parent_session_id"),
        Some(&Value::Null),
        "top-level session should have null parent_session_id, got: {session}"
    );

    // message_count reflects at least the user + assistant messages.
    let message_count = session
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("session object missing 'message_count': {session}"));
    assert!(
        message_count >= 2,
        "expected at least 2 messages (user + assistant), got {message_count}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: session show replays user + assistant messages
// ---------------------------------------------------------------------------

#[test]
fn session_show_replays_user_and_assistant_messages() {
    let runner = CodelRunner::new();
    let provider = MockProvider::start().assistant_text("All done.");
    let runner = runner.with_provider(&provider);

    runner
        .run(&["agent", "run", "Create a greeting file."])
        .assert_success();

    let sessions = list_sessions_json(&runner);
    let id = session_id_of(&sessions[0]).to_string();

    let shown = runner.run(&["session", "show", &id]);
    shown.assert_success();
    let output = shown.stdout();

    // The user prompt must appear.
    assert!(
        output.contains("Create a greeting file."),
        "session show should contain the user prompt; got:\n{output}"
    );
    // The assistant reply must appear.
    assert!(
        output.contains("All done."),
        "session show should contain the assistant reply; got:\n{output}"
    );
    // Role labels must be present.
    assert!(
        output.contains("user"),
        "session show should label the user message; got:\n{output}"
    );
    assert!(
        output.contains("assistant"),
        "session show should label the assistant message; got:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: agent resume appends to the same session
// ---------------------------------------------------------------------------
//
// The mock ordering mechanism counts "role":"tool" messages in the request body.
// A resumed turn replays ALL prior messages from the session (including the
// initial user + assistant pair). The follow-up prompt appears as a new user
// message at the tail, so the second provider receives turn 0 (zero tool
// results) and returns one assistant text turn.
//
// We use two separate providers: one for the initial run and one for the resume,
// so each fresh server starts its turn counter from 0.

#[test]
fn agent_resume_appends_second_turn_with_prior_context() {
    let runner = CodelRunner::new();

    // Turn 1: initial run.
    let provider1 = MockProvider::start().assistant_text("Initial task done.");
    let runner = runner.with_provider(&provider1);

    runner
        .run(&["agent", "run", "Initial task."])
        .assert_success();

    let sessions = list_sessions_json(&runner);
    assert_eq!(sessions.len(), 1, "should have exactly one session");
    let session_id = session_id_of(&sessions[0]).to_string();

    // Turn 2: resume with a follow-up.
    let provider2 = MockProvider::start().assistant_text("Follow-up done.");
    let runner = runner.with_provider(&provider2);

    let resume_result = runner.run(&["agent", "resume", &session_id, "Follow-up task."]);
    resume_result.assert_success();

    // The resumed request must carry the prior conversation messages.
    let requests = provider2.received_requests();
    assert_eq!(
        requests.len(),
        1,
        "resume should make exactly one model request (one text turn), got {requests:?}"
    );
    let request_body = &requests[0];

    // The prior assistant message ("Initial task done.") must be present in the
    // request body that the resumed turn sent to the model.
    assert!(
        request_body.contains("Initial task done."),
        "resumed request should include the prior assistant message; body:\n{request_body}"
    );

    // The new follow-up prompt must also be in the request.
    assert!(
        request_body.contains("Follow-up task."),
        "resumed request should include the follow-up user message; body:\n{request_body}"
    );

    // session list still shows exactly ONE session (resume adds to the same one).
    let sessions_after = list_sessions_json(&runner);
    assert_eq!(
        sessions_after.len(),
        1,
        "agent resume should not create a new session; got {sessions_after:#?}"
    );
    assert_eq!(
        session_id_of(&sessions_after[0]),
        session_id,
        "the resumed session should have the same id"
    );

    // session show now has 4 messages: user1, assistant1, user2, assistant2.
    let shown = runner.run(&["session", "show", &session_id]);
    shown.assert_success();
    let output = shown.stdout();

    // Count message lines (each session show line contains "message" in it).
    let message_lines: Vec<&str> = output
        .lines()
        .filter(|line| line.contains("\tmessage\t"))
        .collect();
    assert_eq!(
        message_lines.len(),
        4,
        "after resume, session should have 4 messages (user, assistant, user, assistant); got:\n{output}"
    );

    // Verify both prompts and both replies are present.
    assert!(
        output.contains("Initial task."),
        "session show should contain the first user prompt; got:\n{output}"
    );
    assert!(
        output.contains("Initial task done."),
        "session show should contain the first assistant reply; got:\n{output}"
    );
    assert!(
        output.contains("Follow-up task."),
        "session show should contain the second user prompt; got:\n{output}"
    );
    assert!(
        output.contains("Follow-up done."),
        "session show should contain the second assistant reply; got:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: agent continue resumes the most-recent session
// ---------------------------------------------------------------------------
//
// `SessionId` uses a process-scoped atomic counter that resets to 1 in every
// new subprocess. Without explicit `--session-id` flags, every `agent run`
// call produces `session-1` and collides into the same session. We therefore
// assign distinct, stable ids so two independent sessions can coexist.
//
// `latest_session_id` picks the session with the highest `created_at`
// (tie-broken by alphabetically-descending id). We give session-b a
// lexicographically higher id so it wins the tie if both run within the same
// millisecond, which matches the production tie-break rule.

#[test]
fn agent_continue_resumes_most_recent_session() {
    let runner = CodelRunner::new();

    // Session A — explicitly named so it does not collide with Session B.
    let provider_a = MockProvider::start().assistant_text("Session A done.");
    let runner = runner.with_provider(&provider_a);
    runner
        .run(&[
            "agent",
            "run",
            "Session A task.",
            "--session-id",
            "session-a",
        ])
        .assert_success();

    // Session B — higher id so it wins any same-millisecond tie-break.
    let provider_b = MockProvider::start().assistant_text("Session B done.");
    let runner = runner.with_provider(&provider_b);
    runner
        .run(&[
            "agent",
            "run",
            "Session B task.",
            "--session-id",
            "session-b",
        ])
        .assert_success();

    let sessions_before = list_sessions_json(&runner);
    assert_eq!(
        sessions_before.len(),
        2,
        "should have two sessions before continue"
    );

    // `agent continue` should resume session-b (newer or higher id).
    let provider_c = MockProvider::start().assistant_text("Continue done.");
    let runner = runner.with_provider(&provider_c);

    let continue_result = runner.run(&["agent", "continue", "Continue task."]);
    continue_result.assert_success();

    // Still exactly 2 sessions: continue appended to an existing one.
    let sessions_after = list_sessions_json(&runner);
    assert_eq!(
        sessions_after.len(),
        2,
        "agent continue must not create a new session; got {sessions_after:#?}"
    );

    // The resumed session now has 4 messages (user, assistant, user, assistant).
    // The other session is untouched with 2 messages.
    let counts: Vec<u64> = sessions_after
        .iter()
        .map(|s| {
            s.get("message_count")
                .and_then(Value::as_u64)
                .expect("message_count must be present")
        })
        .collect();
    let mut sorted_counts = counts.clone();
    sorted_counts.sort_unstable();
    assert_eq!(
        sorted_counts,
        vec![2, 4],
        "after continue, one session has 4 messages and the other has 2; got {counts:?}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5: recency ordering — session list orders most-recent first
// ---------------------------------------------------------------------------
//
// `SessionId` resets to 1 on every subprocess, so two `agent run` calls
// without `--session-id` would collide on `session-1`. We use explicit ids
// so both sessions persist independently. `latest_session_id` picks the
// highest `created_at` (tie-broken by alphabetically-descending id). We name
// the second session with a higher id so it always sorts first regardless of
// clock precision.

#[test]
fn session_list_orders_most_recent_first() {
    let runner = CodelRunner::new();

    // First session — lower id, created first.
    let provider1 = MockProvider::start().assistant_text("First session reply.");
    let runner = runner.with_provider(&provider1);
    runner
        .run(&[
            "agent",
            "run",
            "First session prompt.",
            "--session-id",
            "session-alpha",
        ])
        .assert_success();

    // Second session — higher id, created second (wins on both timestamp and
    // tie-break criteria).
    let provider2 = MockProvider::start().assistant_text("Second session reply.");
    let runner = runner.with_provider(&provider2);
    runner
        .run(&[
            "agent",
            "run",
            "Second session prompt.",
            "--session-id",
            "session-beta",
        ])
        .assert_success();

    let sessions = list_sessions_json(&runner);
    assert_eq!(
        sessions.len(),
        2,
        "should have exactly two sessions, got: {sessions:#?}"
    );

    // Both sessions carry created_at timestamps.
    let ts0 = sessions[0]
        .get("created_at")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("first session missing created_at: {}", sessions[0]));
    let ts1 = sessions[1]
        .get("created_at")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("second session missing created_at: {}", sessions[1]));

    // The list is sorted most-recent first: ts0 >= ts1.
    assert!(
        ts0 >= ts1,
        "session list should be most-recent first: ts0={ts0} ts1={ts1}\nsessions:\n{sessions:#?}"
    );

    // Regardless of timestamp, the session with the higher id (session-beta)
    // must appear before session-alpha, because either it has a newer timestamp
    // or it wins the tie-break. Verify the id ordering invariant.
    let id0 = session_id_of(&sessions[0]);
    let id1 = session_id_of(&sessions[1]);
    assert!(
        id0 >= id1,
        "session list should put the higher-id (or newer) session first; id0={id0} id1={id1}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 6: session list --json structure (field presence and types)
// ---------------------------------------------------------------------------

#[test]
fn session_list_json_fields_are_well_typed() {
    let runner = CodelRunner::new();
    let provider = MockProvider::start().assistant_text("Field check reply.");
    let runner = runner.with_provider(&provider);

    runner
        .run(&["agent", "run", "Field check prompt."])
        .assert_success();

    let sessions = list_sessions_json(&runner);
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];

    // session_id: non-empty string.
    let id = session
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing session_id in {session}"));
    assert!(!id.is_empty());

    // source: non-empty string (comes from `persist_session_records(..., "cli", ...)`).
    let source = session
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing source in {session}"));
    assert!(
        !source.is_empty(),
        "source should be non-empty, got: {source:?}"
    );

    // message_count: non-negative integer.
    let message_count = session
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing message_count in {session}"));
    assert!(message_count >= 2, "message_count should be >= 2");

    // event_count: non-negative integer.
    session
        .get("event_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing event_count in {session}"));

    // created_at: non-zero integer.
    let created_at = session
        .get("created_at")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing created_at in {session}"));
    assert!(created_at > 0, "created_at should be > 0");

    // parent_session_id: null for a top-level session.
    assert_eq!(
        session.get("parent_session_id"),
        Some(&Value::Null),
        "top-level session should have null parent_session_id"
    );
}
