//! A scripted, hermetic mock provider.
//!
//! Wraps an [`httpmock::MockServer`] that answers `POST /chat/completions` with a
//! pre-scripted sequence of OpenAI chat-completions responses.
//!
//! # Ordering mechanism
//!
//! The agent drives a tool loop by re-calling the model after each tool result.
//! Every iteration's request body therefore carries one *more* `"role":"tool"`
//! message than the previous one: the very first request has zero tool results,
//! the request after the first tool runs has one, and so on. We exploit this to
//! serve turns **in order, deterministically and idempotently**: turn `i` is
//! registered with a custom matcher (`when.is_true`) that fires only when the
//! request body contains exactly `i` tool-result messages. This avoids any
//! reliance on mutable call counters inside matchers (which httpmock may evaluate
//! repeatedly or out of order).
//!
//! The tool-call response shape mirrors exactly what the `custom`
//! (OpenAI-compatible) transport produces and parses: `finish_reason:
//! "tool_calls"` with a `tool_calls` array of `function` calls whose `arguments`
//! is a JSON-encoded **string**.

use std::sync::{Arc, Mutex};

use httpmock::{Method::POST, MockServer, prelude::HttpMockRequest};
use serde_json::{Value, json};

/// One scripted model response.
#[derive(Clone)]
enum Turn {
    /// A final assistant message (`finish_reason: "stop"`).
    Text(String),
    /// One or more tool calls in a single assistant turn
    /// (`finish_reason: "tool_calls"`).
    ToolCalls(Vec<(String, Value)>),
}

/// A fluent builder for a scripted conversation served to the real binary.
///
/// Build the *whole* script up front, then attach it to a [`crate::CodelRunner`]
/// via [`crate::CodelRunner::with_provider`] and run. The mocks are installed
/// when the builder is finalized through [`MockProvider::start`]/the chaining
/// methods — each method registers its turn's mock immediately.
///
/// ```ignore
/// let provider = MockProvider::start()
///     .tool_call("create_file", json!({"path": "x.txt", "content": "hi"}))
///     .tool_call("run_command", json!({"program": "echo", "args": ["hi"]}))
///     .assistant_text("done");
/// ```
pub struct MockProvider {
    server: MockServer,
    /// Bodies of requests served so far, recorded in matcher order.
    captured: Arc<Mutex<Vec<String>>>,
    /// Count of turns registered so far, used to assign each turn its index.
    registered: usize,
}

impl MockProvider {
    /// Starts a fresh mock server with an empty script.
    #[must_use]
    pub fn start() -> Self {
        Self {
            server: MockServer::start(),
            captured: Arc::new(Mutex::new(Vec::new())),
            registered: 0,
        }
    }

    /// Queues a turn that returns final assistant text and stops.
    #[must_use]
    pub fn assistant_text(mut self, text: impl Into<String>) -> Self {
        let turn = Turn::Text(text.into());
        self.register(turn);
        self
    }

    /// Queues a turn that returns a single tool call.
    ///
    /// `args` is the tool's JSON argument object (e.g.
    /// `json!({"path": "x.txt", "content": "hi"})`); it is JSON-encoded into the
    /// OpenAI `arguments` string, matching the transport's expectations.
    #[must_use]
    pub fn tool_call(mut self, name: impl Into<String>, args: Value) -> Self {
        let turn = Turn::ToolCalls(vec![(name.into(), args)]);
        self.register(turn);
        self
    }

    /// Queues a turn that returns several tool calls at once.
    #[must_use]
    pub fn tool_calls(mut self, calls: Vec<(String, Value)>) -> Self {
        let turn = Turn::ToolCalls(calls);
        self.register(turn);
        self
    }

    /// The base URL to hand to the binary via `--base-url`.
    #[must_use]
    pub fn base_url(&self) -> String {
        self.server.base_url()
    }

    /// How many `POST /chat/completions` requests the binary has made so far.
    #[must_use]
    pub fn hits(&self) -> usize {
        self.captured.lock().unwrap().len()
    }

    /// The raw request bodies the binary sent, in order. Lets tests assert what
    /// the model was shown (advertised tools, messages, tool results).
    #[must_use]
    pub fn received_requests(&self) -> Vec<String> {
        self.captured.lock().unwrap().clone()
    }

    fn register(&mut self, turn: Turn) {
        let index = self.registered;
        self.registered += 1;
        let body = response_for(&turn);
        let captured = Arc::clone(&self.captured);
        self.server.mock(move |when, then| {
            let captured = Arc::clone(&captured);
            when.method(POST)
                .path("/chat/completions")
                .is_true(move |req: &HttpMockRequest| {
                    let request_body = req.body_string();
                    if count_tool_results(&request_body) == index {
                        // Record on the matching turn so each request is captured
                        // exactly once, in served order.
                        captured.lock().unwrap().push(request_body);
                        true
                    } else {
                        false
                    }
                });
            then.status(200)
                .header("content-type", "application/json")
                .json_body(body);
        });
    }
}

/// Counts `"role":"tool"` messages in a request body, tolerating whitespace
/// variations around the colon.
fn count_tool_results(body: &str) -> usize {
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    compact.matches(r#""role":"tool""#).count()
}

fn response_for(turn: &Turn) -> Value {
    match turn {
        Turn::Text(text) => json!({
            "id": "chatcmpl-e2e",
            "object": "chat.completion",
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": text },
                "finish_reason": "stop"
            }]
        }),
        Turn::ToolCalls(calls) => {
            let tool_calls: Vec<Value> = calls
                .iter()
                .enumerate()
                .map(|(i, (name, args))| {
                    json!({
                        "id": format!("call-{i}-{name}"),
                        "type": "function",
                        "function": {
                            "name": name,
                            // OpenAI encodes arguments as a JSON string.
                            "arguments": serde_json::to_string(args).unwrap_or_default()
                        }
                    })
                })
                .collect();
            json!({
                "id": "chatcmpl-e2e",
                "object": "chat.completion",
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": tool_calls
                    },
                    "finish_reason": "tool_calls"
                }]
            })
        }
    }
}
