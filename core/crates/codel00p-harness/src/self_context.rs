//! Agent self-awareness: a compact "who am I" block injected into context each
//! turn (the way project memory and skills are), plus the read-only
//! `self_describe` tool that returns the same facts as structured JSON.
//!
//! Two facets are covered here (metacognition/self-critique is intentionally
//! deferred to a later verify-before-done loop):
//!
//! 1. **Self-knowledge** — static identity and capabilities: codel00p + version,
//!    provider/model, the tools it actually advertises, the execution backend and
//!    whether it isolates, the permission mode, and the active profile (if any).
//!    Carried in [`AgentSelfContext`], plumbed from the CLI through the builder.
//! 2. **Live run-state** — the current iteration N / max, context-window
//!    used/remaining (when known), and plan progress (steps done / total).
//!    Carried in [`AgentSelfState`], refreshed by the harness each iteration.
//!
//! The harness holds one shared [`AgentSelfHandle`]: the assembler reads it to
//! render the system block, and the `self_describe` tool reads the same handle
//! so an explicit query and the injected block never disagree. The tool can read
//! it because `Tool::execute` cannot see harness internals — the handle is an
//! `Arc` the harness updates and the tool clones.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, tools::Tool, workspace::Workspace};

/// Static identity and capabilities of the agent for this run. Set once at build
/// time from the CLI's resolved options; never changes during the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSelfContext {
    /// Product name, e.g. `codel00p`.
    pub name: String,
    /// Build/version string, e.g. `0.9.0`.
    pub version: String,
    /// Inference provider id, e.g. `openrouter`.
    pub provider: String,
    /// Model id, e.g. `openai/gpt-4o-mini`.
    pub model: String,
    /// Resolved tool-set names enabled for this run, e.g. `read,edit,command`.
    pub tool_sets: Vec<String>,
    /// Execution backend the commands/filesystem run on, e.g. `local`/`docker`.
    pub backend: String,
    /// Whether the execution backend isolates the workspace (docker/ssh).
    pub isolated: bool,
    /// Permission mode in force, e.g. `allow`/`ask`/`deny`.
    pub permission_mode: String,
    /// Active behavior profile, if any (e.g. `tdd`). `None` when unset.
    pub profile: Option<String>,
    /// Whether the identity/capabilities block is injected each turn
    /// (`agent.behavior.self_knowledge`, default on).
    pub self_knowledge: bool,
    /// Whether the live run-state line is included (`agent.behavior.self_state`,
    /// default on).
    pub self_state: bool,
}

impl AgentSelfContext {
    /// A minimal context with both facets enabled. Tests and callers fill in the
    /// fields they care about.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            provider: provider.into(),
            model: model.into(),
            tool_sets: Vec::new(),
            backend: "local".to_string(),
            isolated: false,
            permission_mode: "allow".to_string(),
            profile: None,
            self_knowledge: true,
            self_state: true,
        }
    }

    pub fn with_tool_sets(mut self, tool_sets: Vec<String>) -> Self {
        self.tool_sets = tool_sets;
        self
    }

    pub fn with_backend(mut self, backend: impl Into<String>, isolated: bool) -> Self {
        self.backend = backend.into();
        self.isolated = isolated;
        self
    }

    pub fn with_permission_mode(mut self, permission_mode: impl Into<String>) -> Self {
        self.permission_mode = permission_mode.into();
        self
    }

    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_toggles(mut self, self_knowledge: bool, self_state: bool) -> Self {
        self.self_knowledge = self_knowledge;
        self.self_state = self_state;
        self
    }
}

/// Live run-state, refreshed by the harness before each inference. Every field is
/// optional so unknown facts are simply omitted from the rendered block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSelfState {
    /// Current iteration (1-based) within this turn.
    pub iteration: Option<u32>,
    /// Maximum iterations allowed this turn.
    pub max_iterations: Option<u32>,
    /// Context tokens used so far this turn (accumulated usage), if known.
    pub context_used_tokens: Option<u64>,
    /// Context-window size for the model, if known.
    pub context_window_tokens: Option<u64>,
    /// Plan steps completed, if a plan exists.
    pub plan_completed: Option<usize>,
    /// Total plan steps, if a plan exists.
    pub plan_total: Option<usize>,
}

/// The shared, cloneable handle the harness updates and the `self_describe` tool
/// reads. The static [`AgentSelfContext`] is fixed for the run; the live
/// [`AgentSelfState`] sits behind a mutex so the harness can refresh it each
/// iteration while the tool reads a consistent snapshot.
#[derive(Clone)]
pub struct AgentSelfHandle {
    context: Arc<AgentSelfContext>,
    state: Arc<Mutex<AgentSelfState>>,
}

impl AgentSelfHandle {
    pub fn new(context: AgentSelfContext) -> Self {
        Self {
            context: Arc::new(context),
            state: Arc::new(Mutex::new(AgentSelfState::default())),
        }
    }

    pub fn context(&self) -> &AgentSelfContext {
        &self.context
    }

    /// Replace the live run-state snapshot.
    pub fn set_state(&self, state: AgentSelfState) {
        *self.state.lock().expect("self-state lock") = state;
    }

    /// A snapshot of the current live run-state.
    pub fn state(&self) -> AgentSelfState {
        self.state.lock().expect("self-state lock").clone()
    }
}

/// Renders the static context + live state into a compact system block, honoring
/// the two toggles. Returns `None` when both facets are disabled (no block).
pub struct SelfPromptAssembler;

impl SelfPromptAssembler {
    pub fn assemble(&self, context: &AgentSelfContext, state: &AgentSelfState) -> Option<String> {
        if !context.self_knowledge && !context.self_state {
            return None;
        }

        let mut lines: Vec<String> = Vec::new();

        if context.self_knowledge {
            lines.push(format!(
                "You are {} v{} (provider: {}, model: {}).",
                context.name, context.version, context.provider, context.model
            ));

            let mut caps: Vec<String> = Vec::new();
            if !context.tool_sets.is_empty() {
                caps.push(format!("tools = {}", context.tool_sets.join(",")));
            }
            caps.push(format!(
                "execution backend = {} ({})",
                context.backend,
                if context.isolated {
                    "isolated"
                } else {
                    "not isolated"
                }
            ));
            caps.push(format!("permission mode = {}", context.permission_mode));
            if let Some(profile) = &context.profile {
                caps.push(format!("profile = {profile}"));
            }
            lines.push(format!("Capabilities: {}.", caps.join("; ")));
        }

        if context.self_state {
            let mut parts: Vec<String> = Vec::new();
            if let (Some(iteration), Some(max)) = (state.iteration, state.max_iterations) {
                parts.push(format!("iteration {iteration}/{max}"));
            }
            if let Some(used) = state.context_used_tokens {
                match state.context_window_tokens {
                    Some(window) => parts.push(format!(
                        "context ~{}/{} tokens",
                        compact_tokens(used),
                        compact_tokens(window)
                    )),
                    None => parts.push(format!("context ~{} tokens", compact_tokens(used))),
                }
            }
            if let (Some(completed), Some(total)) = (state.plan_completed, state.plan_total)
                && total > 0
            {
                parts.push(format!("plan {completed}/{total} steps done"));
            }
            if !parts.is_empty() {
                lines.push(format!("Run state: {}.", parts.join("; ")));
            }
        }

        if lines.is_empty() {
            return None;
        }

        lines.push(
            "Use this to reason about what you can and cannot do, and to pace yourself."
                .to_string(),
        );
        Some(lines.join("\n"))
    }
}

/// Render a token count compactly (`41000` -> `41k`), keeping small counts exact.
fn compact_tokens(tokens: u64) -> String {
    if tokens >= 1_000 {
        format!("{}k", tokens / 1_000)
    } else {
        tokens.to_string()
    }
}

/// Read-only tool returning the agent's identity, capabilities, and live
/// run-state as structured JSON. Reads the same [`AgentSelfHandle`] the injected
/// self block is rendered from, so an explicit query and the block agree.
pub struct SelfDescribeTool {
    handle: AgentSelfHandle,
}

impl SelfDescribeTool {
    pub fn new(handle: AgentSelfHandle) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl Tool for SelfDescribeTool {
    fn name(&self) -> &str {
        "self_describe"
    }

    fn description(&self) -> &str {
        "Describe yourself: your identity (codel00p + version), provider/model, \
         the tools you actually have, execution backend and isolation, permission \
         mode, active profile, and your current run state (iteration, context \
         usage, plan progress). Read-only — call it when you need to reason about \
         what you can and cannot do right now."
    }

    fn input_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        _input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let context = self.handle.context();
        let state = self.handle.state();
        Ok(ToolResult::json(json!({
            "identity": {
                "name": context.name,
                "version": context.version,
            },
            "provider": context.provider,
            "model": context.model,
            "capabilities": {
                "tools": context.tool_sets,
                "execution_backend": context.backend,
                "isolated": context.isolated,
                "permission_mode": context.permission_mode,
                "profile": context.profile,
            },
            "run_state": {
                "iteration": state.iteration,
                "max_iterations": state.max_iterations,
                "context_used_tokens": state.context_used_tokens,
                "context_window_tokens": state.context_window_tokens,
                "plan_completed": state.plan_completed,
                "plan_total": state.plan_total,
            },
            "toggles": {
                "self_knowledge": context.self_knowledge,
                "self_state": context.self_state,
            },
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_context() -> AgentSelfContext {
        AgentSelfContext::new("codel00p", "0.9.0", "openrouter", "openai/gpt-4o-mini")
            .with_tool_sets(vec!["read".into(), "edit".into(), "command".into()])
            .with_backend("docker", true)
            .with_permission_mode("allow")
            .with_profile(Some("tdd".into()))
    }

    fn full_state() -> AgentSelfState {
        AgentSelfState {
            iteration: Some(3),
            max_iterations: Some(8),
            context_used_tokens: Some(41_000),
            context_window_tokens: Some(200_000),
            plan_completed: Some(2),
            plan_total: Some(5),
        }
    }

    #[test]
    fn assembles_identity_capabilities_and_run_state() {
        let block = SelfPromptAssembler
            .assemble(&full_context(), &full_state())
            .expect("block present");
        assert!(block.contains(
            "You are codel00p v0.9.0 (provider: openrouter, model: openai/gpt-4o-mini)."
        ));
        assert!(block.contains("tools = read,edit,command"));
        assert!(block.contains("execution backend = docker (isolated)"));
        assert!(block.contains("permission mode = allow"));
        assert!(block.contains("profile = tdd"));
        assert!(block.contains("iteration 3/8"));
        assert!(block.contains("context ~41k/200k tokens"));
        assert!(block.contains("plan 2/5 steps done"));
    }

    #[test]
    fn omits_unknown_fields() {
        let context = AgentSelfContext::new("codel00p", "0.9.0", "p", "m").with_profile(None);
        let state = AgentSelfState {
            iteration: Some(1),
            max_iterations: Some(4),
            // No context info, no plan.
            ..Default::default()
        };
        let block = SelfPromptAssembler.assemble(&context, &state).unwrap();
        assert!(!block.contains("profile ="));
        assert!(!block.contains("context"));
        assert!(!block.contains("plan"));
        assert!(!block.contains("tools =")); // no tool sets configured
        assert!(block.contains("iteration 1/4"));
    }

    #[test]
    fn self_knowledge_toggle_drops_identity_block() {
        let context = full_context().with_toggles(false, true);
        let block = SelfPromptAssembler
            .assemble(&context, &full_state())
            .unwrap();
        assert!(!block.contains("You are codel00p"));
        assert!(!block.contains("Capabilities:"));
        assert!(block.contains("Run state:"));
    }

    #[test]
    fn self_state_toggle_drops_run_state_line() {
        let context = full_context().with_toggles(true, false);
        let block = SelfPromptAssembler
            .assemble(&context, &full_state())
            .unwrap();
        assert!(block.contains("You are codel00p"));
        assert!(!block.contains("Run state:"));
    }

    #[test]
    fn both_toggles_off_yields_no_block() {
        let context = full_context().with_toggles(false, false);
        assert!(
            SelfPromptAssembler
                .assemble(&context, &full_state())
                .is_none()
        );
    }

    #[tokio::test]
    async fn self_describe_tool_returns_identity_capabilities_and_state() {
        let handle = AgentSelfHandle::new(full_context());
        handle.set_state(full_state());
        let tool = SelfDescribeTool::new(handle);
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::new(dir.path()).unwrap();
        let result = tool.execute(&ws, json!({})).await.unwrap();
        let content = result.content();
        assert_eq!(content["identity"]["name"], "codel00p");
        assert_eq!(content["identity"]["version"], "0.9.0");
        assert_eq!(content["provider"], "openrouter");
        assert_eq!(content["model"], "openai/gpt-4o-mini");
        assert_eq!(content["capabilities"]["tools"][1], "edit");
        assert_eq!(content["capabilities"]["execution_backend"], "docker");
        assert_eq!(content["capabilities"]["isolated"], true);
        assert_eq!(content["capabilities"]["permission_mode"], "allow");
        assert_eq!(content["capabilities"]["profile"], "tdd");
        assert_eq!(content["run_state"]["iteration"], 3);
        assert_eq!(content["run_state"]["max_iterations"], 8);
        assert_eq!(content["run_state"]["plan_total"], 5);
        assert_eq!(content["toggles"]["self_knowledge"], true);
    }
}
