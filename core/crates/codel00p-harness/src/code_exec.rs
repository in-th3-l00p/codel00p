//! Code execution: the `execute_code` tool (SOTA programmatic tool calling).
//!
//! Where [`run_pipeline`](crate::pipeline) collapses a *declared, linear* list of
//! tool calls into one inference, `execute_code` lets the model write a short
//! **script** with arbitrary control flow — loops, conditionals, locals,
//! try/catch — whose tool calls are governed *exactly* like direct tool calls.
//! Orchestration moves into the script; authority never does.
//!
//! # Sandbox: closed by construction
//!
//! The runtime is [Rhai](https://rhai.rs), a pure-Rust embeddable scripting
//! language. A *fresh* [`rhai::Engine`] is built per call and we register **only**
//! the tool-bridge functions below — no file, network, or process packages. Rhai
//! ships no ambient I/O in its default surface (no `open`/`read`/`system`), so a
//! script cannot touch the filesystem, network, or processes except through the
//! bound, governed tools. A test asserts a script attempting ambient I/O fails.
//!
//! # Governance: identical to a direct tool call
//!
//! Every bound function dispatches through [`dispatch_tool`](crate::pipeline) —
//! the single governed-dispatch function shared with `run_pipeline` and
//! synthesized capabilities. So each in-script call computes its permission
//! scope, builds a [`PermissionRequest`], asks the [`PermissionPolicy`] to
//! decide, and only then dispatches through the [`ToolRegistry`]. A denied tool
//! raises a catchable Rhai error and is recorded; it never runs. The tool's own
//! declared [`Tool::permission_scope`] is the max scope of its bound tools, so
//! the policy can gate `execute_code` up front too.
//!
//! # Async/sync bridge
//!
//! `Tool::execute` is async; Rhai is synchronous, and the governed dispatch is
//! async. We run the *whole* Rhai evaluation inside [`tokio::task::spawn_blocking`]
//! and have each bound function drive its async dispatch with
//! `Handle::block_on(...)`. This is deadlock-safe precisely because it runs on a
//! blocking thread (off the async worker pool), so blocking the current thread on
//! a future never starves the runtime's workers. See [`bound`].
//!
//! # Resource limits
//!
//! The engine caps operations, call depth, string/array sizes, and — via an
//! `on_progress` callback checked against a wall-clock deadline — total run time,
//! so an infinite loop or a runaway allocation returns a bounded error instead of
//! hanging. The script's return value is converted to JSON and size-capped like
//! any other tool result.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use codel00p_protocol::{EventId, PermissionScope, SessionId, TurnId};
use rhai::{Dynamic, Engine, EvalAltResult, Map as RhaiMap};
use serde_json::{Value, json};
use tokio::runtime::Handle;

use crate::{
    errors::HarnessError,
    event_sink::AgentEventSink,
    events::HarnessEvent,
    permissions::PermissionPolicy,
    pipeline::{dispatch_tool, scope_label},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

// --- Resource limits (size + time) -----------------------------------------
//
// Sane bounds so a script is always bounded in both work and wall-clock. These
// govern the *script machinery*; each tool the script calls is independently
// bounded by its own implementation (e.g. command timeouts, result truncation).

/// Max Rhai operations before the engine aborts. Caps pure-compute loops (an
/// `on_progress` tick fires per operation, which is also where the deadline is
/// checked).
const MAX_OPERATIONS: u64 = 5_000_000;
/// Max nested function-call depth, bounding recursion.
const MAX_CALL_LEVELS: usize = 64;
/// Max size (in bytes/chars) of any single string the script builds.
const MAX_STRING_SIZE: usize = 256 * 1024;
/// Max length of any single array the script builds.
const MAX_ARRAY_SIZE: usize = 100_000;
/// Max nesting depth of map/object literals.
const MAX_MAP_SIZE: usize = 10_000;
/// Wall-clock deadline for one `execute_code` evaluation. Enforced by the
/// `on_progress` callback so even an infinite loop (which never returns to Rust)
/// is interrupted.
const WALL_CLOCK_LIMIT: Duration = Duration::from_secs(5);
/// How often (in operations) to re-check the wall clock. Checking every op would
/// be needless overhead; this samples often enough to bound overshoot tightly.
const DEADLINE_CHECK_EVERY: u64 = 2_048;
/// Max bytes of the JSON-encoded script return value surfaced to the model.
const MAX_RESULT_BYTES: usize = 64 * 1024;
/// Max source length of the script itself.
const MAX_SCRIPT_BYTES: usize = 64 * 1024;

/// Build a registry fragment exposing `execute_code`, backed by `sub_tools` (the
/// tool surface the script may call), `policy` (the per-call gate), and an
/// optional `event_sink` for per-call audit events.
///
/// `sub_tools` should not contain `execute_code`, so a script cannot recursively
/// invoke another script.
pub fn code_execution_tools(
    sub_tools: ToolRegistry,
    policy: Arc<dyn PermissionPolicy>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
) -> ToolRegistry {
    let engine = CodeExecutionEngine::new(Arc::new(sub_tools), policy, event_sink);
    ToolRegistry::new().with_tool(ExecuteCodeTool::new(engine))
}

/// One governed tool call made by a script, for the result summary.
#[derive(Clone)]
struct CallReport {
    tool: String,
    ok: bool,
    scope: &'static str,
}

/// Drives Rhai script evaluation with governed, audited tool dispatch.
#[derive(Clone)]
pub struct CodeExecutionEngine {
    sub_tools: Arc<ToolRegistry>,
    policy: Arc<dyn PermissionPolicy>,
    event_sink: Option<Arc<dyn AgentEventSink>>,
}

impl CodeExecutionEngine {
    pub fn new(
        sub_tools: Arc<ToolRegistry>,
        policy: Arc<dyn PermissionPolicy>,
        event_sink: Option<Arc<dyn AgentEventSink>>,
    ) -> Self {
        Self {
            sub_tools,
            policy,
            event_sink,
        }
    }

    /// The highest permission scope among the sub-tools the script could call, so
    /// the outer gate on `execute_code` is never weaker than what a script can do.
    /// (Mirrors the pipeline/capability `max_scope`.) We cannot statically know
    /// which tools a given script will call, so this is the conservative ceiling:
    /// the max scope across the entire bound tool surface.
    fn max_scope(&self) -> PermissionScope {
        self.sub_tools
            .names()
            .iter()
            .map(|name| {
                // Probe with an empty input; tool scopes are input-independent in
                // practice (a tool's scope is its category), so this yields the
                // tool's category scope.
                self.sub_tools.permission_scope(name, &json!({}))
            })
            .max_by_key(|scope| scope_rank(*scope))
            .unwrap_or(PermissionScope::ReadOnly)
    }

    /// Run `script`. Returns the JSON-converted script return value plus a
    /// summary of the governed tool calls it made. Runs the whole evaluation on a
    /// blocking thread so the synchronous bridge functions can `block_on` the
    /// async governed dispatch without starving the async runtime.
    async fn run(&self, workspace: &Workspace, script: String) -> Result<CodeRun, HarnessError> {
        let handle = Handle::current();
        let sub_tools = self.sub_tools.clone();
        let policy = self.policy.clone();
        let event_sink = self.event_sink.clone();
        let workspace = workspace.clone();

        // Everything captured is Send + Sync (Arc<ToolRegistry>, Arc<dyn
        // PermissionPolicy>, Arc<dyn AgentEventSink>, Workspace, Handle), so the
        // closure is Send and can move onto a blocking thread.
        let join = tokio::task::spawn_blocking(move || {
            let mut engine = build_engine();

            // Shared, mutable record of the governed calls this script makes. The
            // `sync` Rhai feature makes registered closures `Send + Sync`, so we
            // guard the record with a mutex captured by each bound function.
            let calls = Arc::new(std::sync::Mutex::new(Vec::<CallReport>::new()));
            let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

            let bridge = Arc::new(bound::Bridge {
                handle,
                sub_tools,
                policy,
                event_sink,
                workspace,
                calls: calls.clone(),
                counter,
            });
            bound::register(&mut engine, bridge);

            let result = engine.eval::<Dynamic>(&script);
            let reports = calls.lock().unwrap().clone();
            (result, reports)
        });

        let (result, reports) = join.await.map_err(|error| HarnessError::ToolFailed {
            name: "execute_code".to_string(),
            message: format!("script task panicked: {error}"),
        })?;

        let value = match result {
            Ok(dynamic) => dynamic_to_json(dynamic),
            Err(error) => {
                return Err(HarnessError::ToolFailed {
                    name: "execute_code".to_string(),
                    message: format!("script error: {error}"),
                });
            }
        };

        Ok(CodeRun { value, reports })
    }
}

/// The result of one `execute_code` evaluation.
struct CodeRun {
    value: Value,
    reports: Vec<CallReport>,
}

/// Build a fresh, closed Rhai engine with the resource limits applied. No I/O,
/// network, or process packages are registered — only the tool bridge (added by
/// the caller). The `on_progress` callback enforces the wall-clock deadline.
fn build_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_call_levels(MAX_CALL_LEVELS);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_ARRAY_SIZE);
    engine.set_max_map_size(MAX_MAP_SIZE);

    // Abort when the wall clock passes the deadline. `on_progress` fires per
    // operation; returning `Some(_)` aborts the script with that token, so even a
    // tight infinite loop that never returns to Rust is bounded in time.
    let deadline = Instant::now() + WALL_CLOCK_LIMIT;
    engine.on_progress(move |ops| {
        if ops % DEADLINE_CHECK_EVERY == 0 && Instant::now() >= deadline {
            Some(Dynamic::from(
                "execute_code: wall-clock time limit exceeded",
            ))
        } else {
            None
        }
    });

    engine
}

/// Severity ranking for picking the tool's effective permission scope (mirrors
/// `pipeline::scope_rank`).
fn scope_rank(scope: PermissionScope) -> u8 {
    match scope {
        PermissionScope::ReadOnly => 0,
        PermissionScope::MemoryWrite => 1,
        PermissionScope::WorkspaceWrite => 2,
        PermissionScope::Network => 3,
        PermissionScope::Shell => 4,
        PermissionScope::ExternalConnector => 5,
    }
}

/// `execute_code`: run a short script with control flow whose tool calls are
/// governed exactly like direct tool calls.
pub struct ExecuteCodeTool {
    engine: CodeExecutionEngine,
}

impl ExecuteCodeTool {
    pub fn new(engine: CodeExecutionEngine) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl Tool for ExecuteCodeTool {
    fn name(&self) -> &str {
        "execute_code"
    }

    fn description(&self) -> &str {
        "Run a short Rhai script with full control flow (loops, conditionals, \
         variables, try/catch) to orchestrate tool calls in a single inference — \
         use this instead of many round-trips when the logic between calls is \
         dynamic (filter results, branch, aggregate). Call tools from the script \
         with the bound functions: read-only `read_file(path)`, `list_files(dir)`, \
         `search_text(query)`, `find_files(pattern)`, `grep(pattern)`, \
         `repo_map()`; and, when permitted, `create_file(path, content)`, \
         `update_file(path, content)`, `delete_file(path)`, `apply_patch(path, \
         find, replace)`, `run_command(program)`, and the `git_*` tools. There is \
         also a generic \
         `call_tool(name, #{ ...args })`. Each returns a map (the tool's JSON \
         result) and raises a catchable error if the tool fails or is denied by \
         the permission policy — every call is permission-checked and audited \
         exactly like a direct tool call. The script has NO filesystem, network, \
         or process access except through these bound tools. `return` a value \
         (number/string/array/map) to surface a result. The script is bounded in \
         time and operations."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["code"],
            "properties": {
                "code": {
                    "type": "string",
                    "description": "The Rhai script to run."
                }
            }
        })
    }

    /// The tool's declared scope is the max scope of its bound tools, so the
    /// policy can gate `execute_code` up front. Each in-script call is *also*
    /// gated individually at execution time via the shared governed dispatch.
    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        self.engine.max_scope()
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let code = required_string(self.name(), &input, "code")?;
        if code.len() > MAX_SCRIPT_BYTES {
            return Err(HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: format!(
                    "script is {} bytes; the maximum is {MAX_SCRIPT_BYTES}",
                    code.len()
                ),
            });
        }

        let run = self.engine.run(workspace, code.to_string()).await?;

        // Size-cap the surfaced return value like any other tool result.
        let (result_value, truncated) = cap_result(run.value);

        let calls: Vec<Value> = run
            .reports
            .iter()
            .map(|report| {
                json!({
                    "tool": report.tool,
                    "ok": report.ok,
                    "scope": report.scope,
                })
            })
            .collect();
        let completed = run.reports.iter().filter(|r| r.ok).count();

        Ok(ToolResult::json(json!({
            "result": result_value,
            "result_truncated": truncated,
            "calls": calls,
            "call_count": run.reports.len(),
            "calls_succeeded": completed,
        })))
    }
}

/// Cap the JSON-encoded size of the script's return value. Over-budget values are
/// replaced by a string preview so the model still sees the head of the output
/// without flooding context.
fn cap_result(value: Value) -> (Value, bool) {
    let encoded = value.to_string();
    if encoded.len() <= MAX_RESULT_BYTES {
        return (value, false);
    }
    let mut preview: String = encoded.chars().take(MAX_RESULT_BYTES).collect();
    preview.push_str("…[truncated]");
    (Value::String(preview), true)
}

/// Convert a Rhai [`Dynamic`] into a [`serde_json::Value`].
///
/// Rhai's `serde` feature could do this, but a direct conversion keeps the
/// dependency surface minimal and handles the small set of types scripts return
/// (numbers, strings, bools, arrays, maps, unit). Unsupported exotic types
/// stringify, which is safe for a result payload.
pub(crate) fn dynamic_to_json(value: Dynamic) -> Value {
    if value.is_unit() {
        return Value::Null;
    }
    if value.is_bool() {
        return Value::Bool(value.as_bool().unwrap_or(false));
    }
    if value.is_int() {
        return json!(value.as_int().unwrap_or(0));
    }
    if value.is_float() {
        return serde_json::Number::from_f64(value.as_float().unwrap_or(0.0))
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if value.is_string() {
        return Value::String(value.into_string().unwrap_or_default());
    }
    if value.is_array() {
        let array = value.cast::<rhai::Array>();
        return Value::Array(array.into_iter().map(dynamic_to_json).collect());
    }
    if value.is_map() {
        let map = value.cast::<RhaiMap>();
        let object = map
            .into_iter()
            .map(|(k, v)| (k.to_string(), dynamic_to_json(v)))
            .collect();
        return Value::Object(object);
    }
    // Fallback: stringify anything else (e.g. char, timestamp).
    Value::String(value.to_string())
}

/// Convert a [`serde_json::Value`] into a Rhai [`Dynamic`] so a tool result can
/// be handed back to the script as a native map/array/scalar.
pub(crate) fn json_to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                Dynamic::from(f)
            } else {
                Dynamic::UNIT
            }
        }
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Array(items) => {
            let array: rhai::Array = items.iter().map(json_to_dynamic).collect();
            Dynamic::from(array)
        }
        Value::Object(map) => {
            let mut rmap = RhaiMap::new();
            for (k, v) in map {
                rmap.insert(k.as_str().into(), json_to_dynamic(v));
            }
            Dynamic::from(rmap)
        }
    }
}

/// The bound tool functions (the bridge) and the async/sync machinery.
mod bound {
    use super::*;

    /// Captured state every bound function shares. All fields are `Send + Sync`
    /// so the bridge can live in registered Rhai closures (with the `sync`
    /// feature) and be driven from the blocking thread.
    pub(super) struct Bridge {
        pub(super) handle: Handle,
        pub(super) sub_tools: Arc<ToolRegistry>,
        pub(super) policy: Arc<dyn PermissionPolicy>,
        pub(super) event_sink: Option<Arc<dyn AgentEventSink>>,
        pub(super) workspace: Workspace,
        pub(super) calls: Arc<std::sync::Mutex<Vec<CallReport>>>,
        pub(super) counter: Arc<std::sync::atomic::AtomicU64>,
    }

    impl Bridge {
        /// Dispatch one tool call through the **shared governed dispatch**
        /// (`dispatch_tool`), emit the same audit events a direct call emits, and
        /// record it in the call summary. On denial/failure, return a Rhai error
        /// the script can `try`/`catch`. This is the single choke point through
        /// which every in-script tool call passes.
        fn call(&self, tool: &str, input: Value) -> Result<Dynamic, Box<EvalAltResult>> {
            let seq = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let request_id = format!("execute_code-call-{seq}");
            let scope = self.sub_tools.permission_scope(tool, &input);

            // Audit: the call was requested (same event a direct call emits).
            self.emit(HarnessEvent::ToolCallRequested {
                event_id: EventId::new(),
                session_id: audit_session(),
                turn_id: audit_turn(),
                tool_name: tool.to_string(),
            });
            // Audit: the permission request (same event + request_id scheme).
            self.emit(HarnessEvent::PermissionRequested {
                event_id: EventId::new(),
                session_id: audit_session(),
                turn_id: audit_turn(),
                tool_name: tool.to_string(),
                request_id: request_id.clone(),
                scope,
            });

            // Run the async governed dispatch on this blocking thread. Safe
            // because we are NOT on an async worker — blocking here cannot starve
            // the runtime's worker pool (that is the whole point of running the
            // evaluation under `spawn_blocking`).
            let outcome = self.handle.block_on(dispatch_tool(
                &self.sub_tools,
                self.policy.as_ref(),
                &self.workspace,
                tool,
                input,
                request_id.clone(),
            ));

            let outcome = match outcome {
                Ok(outcome) => outcome,
                // A policy *infrastructure* error (not a denial). Surface it.
                Err(error) => {
                    self.record(tool, false, scope);
                    self.emit(HarnessEvent::ToolCallFailed {
                        event_id: EventId::new(),
                        session_id: audit_session(),
                        turn_id: audit_turn(),
                        tool_name: tool.to_string(),
                        message: error.to_string(),
                    });
                    return Err(rhai_error(format!("{tool}: {error}")));
                }
            };

            self.record(tool, outcome.ok, scope);

            if outcome.ok {
                self.emit(HarnessEvent::ToolCallCompleted {
                    event_id: EventId::new(),
                    session_id: audit_session(),
                    turn_id: audit_turn(),
                    tool_name: tool.to_string(),
                });
                Ok(json_to_dynamic(&outcome.value))
            } else {
                // Distinguish a denial from a tool failure for the audit trail,
                // mirroring the turn loop (PermissionDenied vs ToolCallFailed).
                let message = outcome
                    .value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("tool call failed")
                    .to_string();
                let denied = outcome.value.get("error_kind").and_then(Value::as_str)
                    == Some("permission_denied");
                if denied {
                    self.emit(HarnessEvent::PermissionDenied {
                        event_id: EventId::new(),
                        session_id: audit_session(),
                        turn_id: audit_turn(),
                        tool_name: tool.to_string(),
                        request_id,
                        message: message.clone(),
                    });
                } else {
                    self.emit(HarnessEvent::ToolCallFailed {
                        event_id: EventId::new(),
                        session_id: audit_session(),
                        turn_id: audit_turn(),
                        tool_name: tool.to_string(),
                        message: message.clone(),
                    });
                }
                Err(rhai_error(format!("{tool}: {message}")))
            }
        }

        fn record(&self, tool: &str, ok: bool, scope: PermissionScope) {
            self.calls.lock().unwrap().push(CallReport {
                tool: tool.to_string(),
                ok,
                scope: scope_label(scope),
            });
        }

        fn emit(&self, event: HarnessEvent) {
            if let Some(sink) = &self.event_sink {
                // The sink is async; emit it on the runtime from this blocking
                // thread (same block_on rationale as the dispatch).
                self.handle.block_on(sink.emit(&event));
            }
        }
    }

    /// Register the generic `call_tool` plus ergonomic named wrappers on `engine`.
    /// Every wrapper funnels through `Bridge::call`, so they share one governed,
    /// audited dispatch path.
    pub(super) fn register(engine: &mut Engine, bridge: Arc<Bridge>) {
        // Generic escape hatch: call_tool(name, #{ ...args }).
        let b = bridge.clone();
        engine.register_fn(
            "call_tool",
            move |name: &str, args: RhaiMap| -> Result<Dynamic, Box<EvalAltResult>> {
                b.call(name, map_to_json(args))
            },
        );
        // call_tool(name) with no args.
        let b = bridge.clone();
        engine.register_fn(
            "call_tool",
            move |name: &str| -> Result<Dynamic, Box<EvalAltResult>> { b.call(name, json!({})) },
        );

        // --- read-only wrappers (input field names match each tool's schema) ---
        register_1(engine, &bridge, "read_file", "read_file", "path");
        register_1(engine, &bridge, "list_files", "list_files", "path");
        register_1(engine, &bridge, "search_text", "search_text", "query");
        register_1(engine, &bridge, "find_files", "find_files", "pattern");
        register_1(engine, &bridge, "grep", "grep", "pattern");

        // list_files() / repo_map() with no argument.
        register_0(engine, &bridge, "list_files", "list_files");
        register_0(engine, &bridge, "repo_map", "repo_map");

        // --- mutating wrappers (governed per call; denied if policy refuses) ---
        let b = bridge.clone();
        engine.register_fn(
            "create_file",
            move |path: &str, content: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                b.call("create_file", json!({ "path": path, "content": content }))
            },
        );
        // update_file replaces a file's whole content.
        let b = bridge.clone();
        engine.register_fn(
            "update_file",
            move |path: &str, content: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                b.call("update_file", json!({ "path": path, "content": content }))
            },
        );
        register_1(engine, &bridge, "delete_file", "delete_file", "path");
        // apply_patch does a tolerant find/replace; the ergonomic form wraps a
        // single change. For multiple changes or `replace_all`, use
        // call_tool("apply_patch", #{ "changes": [...] }).
        let b = bridge.clone();
        engine.register_fn(
            "apply_patch",
            move |path: &str, find: &str, replace: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                b.call(
                    "apply_patch",
                    json!({ "changes": [{ "path": path, "find": find, "replace": replace }] }),
                )
            },
        );
        // run_command(program) runs through the configured TerminalBackend
        // (local/docker/ssh), so in-script commands inherit that isolation. For
        // args/cwd/timeout use call_tool("run_command", #{ "program": ..., "args": [...] }).
        register_1(engine, &bridge, "run_command", "run_command", "program");

        // --- git wrappers (mirror the git_* tool names) ---
        register_0(engine, &bridge, "git_status", "git_status");
        register_0(engine, &bridge, "git_diff", "git_diff");
        register_0(engine, &bridge, "git_log", "git_log");
        let b = bridge.clone();
        engine.register_fn(
            "git_commit",
            move |message: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                b.call("git_commit", json!({ "message": message }))
            },
        );
    }

    /// Register a zero-argument wrapper named `fn_name` that calls `tool` with no
    /// input.
    fn register_0(engine: &mut Engine, bridge: &Arc<Bridge>, fn_name: &str, tool: &'static str) {
        let b = bridge.clone();
        engine.register_fn(fn_name, move || -> Result<Dynamic, Box<EvalAltResult>> {
            b.call(tool, json!({}))
        });
    }

    /// Register a one-string-argument wrapper named `fn_name` that calls `tool`
    /// with `{ key: arg }`.
    fn register_1(
        engine: &mut Engine,
        bridge: &Arc<Bridge>,
        fn_name: &str,
        tool: &'static str,
        key: &'static str,
    ) {
        let b = bridge.clone();
        engine.register_fn(
            fn_name,
            move |arg: &str| -> Result<Dynamic, Box<EvalAltResult>> {
                b.call(tool, json!({ key: arg }))
            },
        );
    }

    /// Map a Rhai argument map into JSON tool input.
    fn map_to_json(map: RhaiMap) -> Value {
        let object = map
            .into_iter()
            .map(|(k, v)| (k.to_string(), dynamic_to_json(v)))
            .collect();
        Value::Object(object)
    }

    fn rhai_error(message: String) -> Box<EvalAltResult> {
        Box::new(EvalAltResult::ErrorRuntime(
            Dynamic::from(message),
            rhai::Position::NONE,
        ))
    }

    /// Audit session/turn ids for in-script calls. `execute_code` runs inside a
    /// tool, which has no turn context, so it tags its audit events with a
    /// dedicated, stable id (matching the `request_id` prefix).
    fn audit_session() -> SessionId {
        SessionId::from_static("execute_code")
    }
    fn audit_turn() -> TurnId {
        TurnId::from_static("execute_code")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{
        AllowAllPermissionPolicy, PermissionDecision, PermissionMode, PermissionRequest,
    };
    use std::sync::Mutex;

    /// A policy that denies any tool whose name is in `deny`.
    struct DenyList {
        deny: Vec<&'static str>,
    }

    #[async_trait]
    impl PermissionPolicy for DenyList {
        async fn decide(
            &self,
            request: PermissionRequest,
        ) -> Result<PermissionDecision, HarnessError> {
            if self.deny.contains(&request.tool_name()) {
                Ok(PermissionDecision::deny(
                    request.id(),
                    PermissionMode::Deny,
                    "denied by test policy",
                ))
            } else {
                Ok(PermissionDecision::allow(
                    request.id(),
                    PermissionMode::Allow,
                ))
            }
        }
    }

    /// Records emitted events for audit assertions.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<HarnessEvent>>,
    }

    #[async_trait]
    impl AgentEventSink for RecordingSink {
        async fn emit(&self, event: &HarnessEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, content).unwrap();
        }
        let ws = Workspace::new(dir.path()).unwrap();
        (dir, ws)
    }

    fn tool_with(
        policy: Arc<dyn PermissionPolicy>,
        sink: Option<Arc<dyn AgentEventSink>>,
    ) -> ExecuteCodeTool {
        let sub =
            ToolRegistry::read_only_defaults().with_registry(ToolRegistry::editing_defaults());
        ExecuteCodeTool::new(CodeExecutionEngine::new(Arc::new(sub), policy, sink))
    }

    fn tool(policy: Arc<dyn PermissionPolicy>) -> ExecuteCodeTool {
        tool_with(policy, None)
    }

    #[tokio::test]
    async fn loop_and_conditional_aggregate_a_value() {
        let (_dir, ws) = workspace_with(&[
            ("a.txt", "alpha\n"),
            ("b.txt", "beta\n"),
            ("c.txt", "gamma\n"),
        ]);
        let exec = tool(Arc::new(AllowAllPermissionPolicy));

        // A script with a loop + conditional that reads several files and returns
        // an aggregated map. This is the thing run_pipeline cannot express.
        let result = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        let names = ["a.txt", "b.txt", "c.txt", "missing.txt"];
                        let total = 0;
                        let found = [];
                        for name in names {
                            try {
                                let r = read_file(name);
                                total += r.content.len();
                                if r.content.contains("beta") {
                                    found.push(name);
                                }
                            } catch (err) {
                                // skip missing files
                            }
                        }
                        #{ "total_bytes": total, "has_beta": found }
                    "#
                }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["result"]["has_beta"][0], "b.txt");
        assert_eq!(content["result"]["total_bytes"], 6 + 5 + 6);
        // Four read attempts were governed (the missing one failed, not denied).
        assert_eq!(content["call_count"], 4);
        assert_eq!(content["calls_succeeded"], 3);
        assert_eq!(content["calls"][0]["tool"], "read_file");
        assert_eq!(content["calls"][0]["scope"], "read_only");
    }

    #[tokio::test]
    async fn denied_mutation_is_refused_per_call_and_workspace_unchanged() {
        let (dir, ws) = workspace_with(&[("a.txt", "x")]);
        let exec = tool(Arc::new(DenyList {
            deny: vec!["create_file"],
        }));

        // The script tries to create a file; the per-call gate denies it. The
        // raise is caught and reported, proving authority stayed with the policy.
        let result = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        let denied = false;
                        try {
                            create_file("new.txt", "should not exist");
                        } catch (err) {
                            denied = true;
                        }
                        denied
                    "#
                }),
            )
            .await
            .unwrap();

        assert_eq!(result.content()["result"], true);
        // The denied write never happened.
        assert!(!dir.path().join("new.txt").exists());
        // The denied call is recorded as not-ok.
        assert_eq!(result.content()["calls"][0]["tool"], "create_file");
        assert_eq!(result.content()["calls"][0]["ok"], false);
    }

    #[tokio::test]
    async fn allowed_mutation_actually_writes_the_file() {
        let (dir, ws) = workspace_with(&[]);
        let exec = tool(Arc::new(AllowAllPermissionPolicy));

        let result = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        create_file("out.txt", "written by script");
                        "ok"
                    "#
                }),
            )
            .await
            .unwrap();

        assert_eq!(result.content()["result"], "ok");
        let written = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();
        assert_eq!(written, "written by script");
        assert_eq!(result.content()["calls"][0]["scope"], "workspace_write");
    }

    #[tokio::test]
    async fn sandbox_has_no_ambient_io_builtins() {
        let (_dir, ws) = workspace_with(&[("secret.txt", "TOPSECRET")]);
        let exec = tool(Arc::new(AllowAllPermissionPolicy));

        // None of these ambient-I/O names exist in the closed engine: the script
        // fails (function-not-found / parse) rather than touching the host.
        for snippet in [
            r#"open("secret.txt")"#,
            r#"system("ls")"#,
            r#"read("/etc/passwd")"#,
            r#"import "std" as s; s::run()"#,
        ] {
            let result = exec.execute(&ws, json!({ "code": snippet })).await;
            assert!(
                result.is_err(),
                "ambient I/O snippet should fail in the closed sandbox: {snippet}"
            );
        }
    }

    #[tokio::test]
    async fn infinite_loop_is_bounded_not_a_hang() {
        let (_dir, ws) = workspace_with(&[]);
        let exec = tool(Arc::new(AllowAllPermissionPolicy));

        // A pure-compute infinite loop must hit the operation/time cap and return
        // a bounded error instead of hanging the test.
        let result = exec
            .execute(
                &ws,
                json!({ "code": "let i = 0; while true { i += 1; } i" }),
            )
            .await;
        assert!(
            result.is_err(),
            "an infinite loop must be aborted by the resource limits"
        );
    }

    #[tokio::test]
    async fn oversized_result_is_capped() {
        let (_dir, ws) = workspace_with(&[]);
        let exec = tool(Arc::new(AllowAllPermissionPolicy));

        // Build a string larger than MAX_RESULT_BYTES and return it.
        let result = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        let s = "";
                        let chunk = "0123456789";
                        for i in 0..20000 { s += chunk; }
                        s
                    "#
                }),
            )
            .await
            .unwrap();

        assert_eq!(result.content()["result_truncated"], true);
        let surfaced = result.content()["result"].as_str().unwrap();
        assert!(surfaced.ends_with("…[truncated]"));
        assert!(surfaced.len() <= MAX_RESULT_BYTES + 32);
    }

    #[tokio::test]
    async fn emits_audit_events_per_in_script_call() {
        let (_dir, ws) = workspace_with(&[("a.txt", "x")]);
        let sink = Arc::new(RecordingSink::default());
        let exec = tool_with(
            Arc::new(DenyList {
                deny: vec!["create_file"],
            }),
            Some(sink.clone()),
        );

        let _ = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        read_file("a.txt");
                        try { create_file("b.txt", "y"); } catch (e) {}
                    "#
                }),
            )
            .await
            .unwrap();

        let events = sink.events.lock().unwrap();
        // Same per-call audit events a direct call emits: requested + permission
        // for both; completed for the allowed read; denied for the create.
        let requested = events
            .iter()
            .filter(|e| matches!(e, HarnessEvent::ToolCallRequested { .. }))
            .count();
        assert_eq!(requested, 2, "one ToolCallRequested per in-script call");
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::ToolCallCompleted { tool_name, .. } if tool_name == "read_file"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            HarnessEvent::PermissionDenied { tool_name, .. } if tool_name == "create_file"
        )));
    }

    #[tokio::test]
    async fn find_files_and_grep_use_pattern_field() {
        let (_dir, ws) = workspace_with(&[("src/a.rs", "fn needle() {}"), ("README.md", "hi")]);
        let exec = tool(Arc::new(AllowAllPermissionPolicy));

        // Exercises the corrected `pattern` field for both find_files and grep.
        let result = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        let rs = find_files("**/*.rs");
                        let hits = grep("needle");
                        #{ "rs_count": rs.files.len(), "grep_count": hits.matches.len() }
                    "#
                }),
            )
            .await
            .unwrap();
        let content = result.content();
        assert_eq!(content["result"]["rs_count"], 1);
        assert_eq!(content["result"]["grep_count"], 1);
    }

    #[tokio::test]
    async fn run_command_dispatches_through_the_terminal_backend() {
        // A command tool set is available, so run_command is bound and dispatches
        // through the configured TerminalBackend (LocalBackend here).
        let (_dir, ws) = workspace_with(&[]);
        let sub =
            ToolRegistry::read_only_defaults().with_registry(ToolRegistry::command_defaults());
        let exec = ExecuteCodeTool::new(CodeExecutionEngine::new(
            Arc::new(sub),
            Arc::new(AllowAllPermissionPolicy),
            None,
        ));

        let result = exec
            .execute(
                &ws,
                json!({
                    "code": r#"
                        let out = call_tool("run_command", #{ "program": "echo", "args": ["hello-from-script"] });
                        out
                    "#
                }),
            )
            .await
            .unwrap();
        let content = result.content();
        // The command ran and its (capped) stdout came back through the bridge.
        let stdout = content["result"]["stdout"].as_str().unwrap_or("");
        assert!(
            stdout.contains("hello-from-script"),
            "run_command stdout should round-trip, got {content:?}"
        );
        assert_eq!(content["calls"][0]["scope"], "shell");
    }

    #[test]
    fn declared_scope_is_max_of_bound_tools() {
        let exec = tool(Arc::new(AllowAllPermissionPolicy));
        // read-only + editing defaults => create_file (WorkspaceWrite) is the max.
        assert_eq!(
            exec.permission_scope(&json!({})),
            PermissionScope::WorkspaceWrite
        );
    }

    #[test]
    fn json_round_trips_through_dynamic() {
        let value = json!({ "a": 1, "b": ["x", true, 2.5], "c": null });
        let back = dynamic_to_json(json_to_dynamic(&value));
        assert_eq!(back["a"], 1);
        assert_eq!(back["b"][0], "x");
        assert_eq!(back["b"][1], true);
        assert_eq!(back["c"], Value::Null);
    }
}
