use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    background::BackgroundProcesses,
    commands::{ProcessKillTool, ProcessListTool, ProcessOutputTool, RunCommandTool},
    editing::{ApplyPatchTool, CreateFileTool, DeleteFileTool, UpdateFileTool},
    errors::HarnessError,
    find::{FindFilesTool, GrepTool},
    git::{GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool},
    planning::{PlanStore, UpdatePlanTool},
    repo_map::RepoMapTool,
    tool_result::ToolResult,
    tools::{ListFilesTool, ReadFileTool, SearchTextTool, Tool, ToolSpec},
    web::web_tools,
    workspace::Workspace,
};

/// Name of the synthetic tool that searches the deferred (hidden) tool catalog.
pub const TOOL_SEARCH: &str = "tool_search";
/// Name of the synthetic tool that returns full schemas for hidden tools.
pub const TOOL_DESCRIBE: &str = "tool_describe";
/// Default and ceiling for the number of hits `tool_search` returns.
const DEFAULT_SEARCH_LIMIT: usize = 25;
const MAX_SEARCH_LIMIT: usize = 100;

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
    /// Names of registered tools that are executable but kept out of the
    /// advertised tool set (progressive disclosure). The model discovers them
    /// through the synthetic `tool_search` / `tool_describe` tools and can then
    /// call them by name like any other tool.
    deferred: BTreeSet<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_only_defaults() -> Self {
        Self::new()
            .with_tool(ListFilesTool)
            .with_tool(ReadFileTool)
            .with_tool(SearchTextTool)
            .with_tool(FindFilesTool)
            .with_tool(GrepTool)
            .with_tool(RepoMapTool)
    }

    pub fn editing_defaults() -> Self {
        Self::new()
            .with_tool(ApplyPatchTool)
            .with_tool(CreateFileTool)
            .with_tool(DeleteFileTool)
            .with_tool(UpdateFileTool)
    }

    pub fn command_defaults() -> Self {
        // All four command tools share one process store so `process_output` /
        // `process_list` / `process_kill` see what `run_command` spawned.
        let processes = BackgroundProcesses::new();
        Self::new()
            .with_tool(RunCommandTool::new(processes.clone()))
            .with_tool(ProcessOutputTool::new(processes.clone()))
            .with_tool(ProcessListTool::new(processes.clone()))
            .with_tool(ProcessKillTool::new(processes))
    }

    pub fn git_defaults() -> Self {
        Self::new()
            .with_tool(GitCommitTool)
            .with_tool(GitDiffTool)
            .with_tool(GitLogTool)
            .with_tool(GitStatusTool)
    }

    /// Web tools (`web_fetch`, `web_search`) gated behind `PermissionScope::Network`.
    pub fn web_defaults() -> Self {
        Self::new().with_registry(web_tools())
    }

    /// The planning tool (`update_plan`), backed by a fresh in-memory plan store.
    pub fn planning_defaults() -> Self {
        Self::new().with_tool(UpdatePlanTool::new(PlanStore::new()))
    }

    pub fn with_tool<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        let name = tool.name().to_string();
        self.deferred.remove(&name);
        self.tools.insert(name, Arc::new(tool));
        self
    }

    /// Insert an already type-erased tool.
    ///
    /// This is the entry point used when tools are contributed dynamically (for
    /// example by a plugin) rather than by a statically typed `with_tool` call.
    /// A later insertion with the same tool name replaces an earlier one, so
    /// callers that fold several sources in sequence get last-writer-wins.
    pub fn with_tool_arc(mut self, tool: Arc<dyn Tool>) -> Self {
        let name = tool.name().to_string();
        self.deferred.remove(&name);
        self.tools.insert(name, tool);
        self
    }

    pub fn with_registry(mut self, registry: Self) -> Self {
        for name in registry.tools.keys() {
            self.deferred.remove(name);
        }
        // Tools deferred in the incoming registry stay deferred unless they are
        // also advertised there.
        for name in &registry.deferred {
            if !self.tools.contains_key(name) {
                self.deferred.insert(name.clone());
            }
        }
        self.tools.extend(registry.tools);
        self
    }

    /// Insert a type-erased tool that is executable but **not advertised** in the
    /// prompt. The model reaches it through `tool_search` / `tool_describe`.
    pub fn with_deferred_tool_arc(mut self, tool: Arc<dyn Tool>) -> Self {
        let name = tool.name().to_string();
        self.deferred.insert(name.clone());
        self.tools.insert(name, tool);
        self
    }

    /// Fold every tool from `registry` in as deferred (hidden) tools. Used to
    /// keep a large MCP / plugin tool set out of the prompt while still callable.
    pub fn with_deferred_registry(mut self, registry: Self) -> Self {
        for (name, tool) in registry.tools {
            self.deferred.insert(name.clone());
            self.tools.insert(name, tool);
        }
        self
    }

    /// Defer (hide) every currently-registered tool whose name is not in
    /// `keep_advertised`, when the total number of tools exceeds `threshold`.
    ///
    /// This is the progressive-disclosure entry point: small tool sets are left
    /// fully advertised (no behavior change), while large ones collapse down to
    /// the kept core plus the `tool_search` / `tool_describe` pair.
    pub fn with_progressive_disclosure(
        mut self,
        threshold: usize,
        keep_advertised: &[&str],
    ) -> Self {
        if self.tools.len() <= threshold {
            return self;
        }
        let keep: BTreeSet<&str> = keep_advertised.iter().copied().collect();
        for name in self.tools.keys() {
            if !keep.contains(name.as_str()) {
                self.deferred.insert(name.clone());
            }
        }
        self
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Names of tools that are registered but hidden behind tool search.
    pub fn deferred_names(&self) -> Vec<String> {
        self.deferred.iter().cloned().collect()
    }

    /// Every registered tool's model-facing definition, in stable name order.
    /// This is the full catalog regardless of disclosure state.
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools
            .values()
            .map(|tool| ToolSpec::from_tool(tool.as_ref()))
            .collect()
    }

    /// The tool definitions actually advertised to the provider this turn.
    ///
    /// With no deferred tools this equals [`Self::specs`]. When some tools are
    /// deferred, their schemas are withheld and the synthetic `tool_search` /
    /// `tool_describe` tools are advertised instead, so the model can discover
    /// and load the hidden ones on demand without paying their prompt cost.
    pub fn advertised_specs(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<ToolSpec> = self
            .tools
            .iter()
            .filter(|(name, _)| !self.deferred.contains(*name))
            .map(|(_, tool)| ToolSpec::from_tool(tool.as_ref()))
            .collect();

        if !self.deferred.is_empty() {
            if !self.tools.contains_key(TOOL_SEARCH) {
                specs.push(tool_search_spec(self.deferred.len()));
            }
            if !self.tools.contains_key(TOOL_DESCRIBE) {
                specs.push(tool_describe_spec());
            }
        }
        specs
    }

    /// Whether `name` is one of the synthetic disclosure tools, active because
    /// some tools are deferred and the name is not a real registered tool.
    fn is_meta_tool(&self, name: &str) -> bool {
        !self.deferred.is_empty()
            && !self.tools.contains_key(name)
            && (name == TOOL_SEARCH || name == TOOL_DESCRIBE)
    }

    pub fn is_concurrency_safe(&self, name: &str, input: &Value) -> bool {
        if self.is_meta_tool(name) {
            return true;
        }
        self.tools
            .get(name)
            .map(|tool| tool.is_concurrency_safe(input))
            .unwrap_or(false)
    }

    pub fn permission_scope(&self, name: &str, input: &Value) -> PermissionScope {
        if self.is_meta_tool(name) {
            return PermissionScope::ReadOnly;
        }
        self.tools
            .get(name)
            .map(|tool| tool.permission_scope(input))
            .unwrap_or(PermissionScope::ExternalConnector)
    }

    pub async fn execute(
        &self,
        name: &str,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        // Synthetic disclosure tools are answered from the catalog itself, before
        // the normal tool lookup, so they need no registered backing tool.
        if self.is_meta_tool(name) {
            return match name {
                TOOL_SEARCH => self.run_tool_search(input),
                TOOL_DESCRIBE => self.run_tool_describe(input),
                _ => unreachable!("is_meta_tool gates these names"),
            };
        }

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| HarnessError::ToolNotFound {
                name: name.to_string(),
            })?;

        // Validate arguments against the tool's schema before any side effect, so
        // a malformed call becomes a self-correctable error rather than an ad-hoc
        // failure mid-execution.
        crate::validation::validate_tool_input(name, &tool.input_schema(), &input)?;

        tool.execute(workspace, input).await
    }

    /// Search the deferred-tool catalog by free-text query over names and
    /// descriptions. With no query, lists the hidden tools alphabetically.
    fn run_tool_search(&self, input: Value) -> Result<ToolResult, HarnessError> {
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let terms: Vec<&str> = query.split_whitespace().collect();

        let mut scored: Vec<(i64, &str, &str)> = Vec::new();
        for name in &self.deferred {
            let Some(tool) = self.tools.get(name) else {
                continue;
            };
            let description = tool.description();
            let score = score_match(name, description, &terms);
            if score < 0 {
                continue;
            }
            scored.push((score, name.as_str(), description));
        }
        // Highest score first, then alphabetical by name for stable output.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        let total_matches = scored.len();

        let tools: Vec<Value> = scored
            .into_iter()
            .take(limit)
            .map(|(_, name, description)| json!({ "name": name, "description": description }))
            .collect();

        Ok(ToolResult::json(json!({
            "tools": tools,
            "total_matches": total_matches,
            "deferred_total": self.deferred.len(),
            "truncated": total_matches > limit,
        })))
    }

    /// Return the full description and input schema for the requested tool names.
    fn run_tool_describe(&self, input: Value) -> Result<ToolResult, HarnessError> {
        let names = input
            .get("names")
            .and_then(Value::as_array)
            .ok_or_else(|| HarnessError::InvalidToolInput {
                name: TOOL_DESCRIBE.to_string(),
                message: "missing array field `names`".to_string(),
            })?;
        if names.is_empty() {
            return Err(HarnessError::InvalidToolInput {
                name: TOOL_DESCRIBE.to_string(),
                message: "`names` must not be empty".to_string(),
            });
        }

        let mut described = Vec::new();
        for name in names {
            let Some(name) = name.as_str() else {
                return Err(HarnessError::InvalidToolInput {
                    name: TOOL_DESCRIBE.to_string(),
                    message: "`names` must be an array of strings".to_string(),
                });
            };
            match self.tools.get(name) {
                Some(tool) => described.push(json!({
                    "name": name,
                    "description": tool.description(),
                    "input_schema": tool.input_schema(),
                })),
                None => described.push(json!({
                    "name": name,
                    "error": "unknown tool",
                })),
            }
        }

        Ok(ToolResult::json(json!({ "tools": described })))
    }
}

/// Score a tool against query terms. `-1` means "filtered out" (a non-empty
/// query with no term matching); `0` means "no query, include in listing".
fn score_match(name: &str, description: &str, terms: &[&str]) -> i64 {
    if terms.is_empty() {
        return 0;
    }
    let name_lower = name.to_ascii_lowercase();
    let description_lower = description.to_ascii_lowercase();
    let mut score = 0i64;
    let mut matched_any = false;
    for term in terms {
        if name_lower.contains(term) {
            // Name hits weigh more than description hits, with an exact-name
            // match weighing most.
            score += if name_lower == *term { 100 } else { 10 };
            matched_any = true;
        }
        if description_lower.contains(term) {
            score += 1;
            matched_any = true;
        }
    }
    if matched_any { score } else { -1 }
}

/// Build the model-facing spec for the `tool_search` disclosure tool.
fn tool_search_spec(deferred_count: usize) -> ToolSpec {
    ToolSpec::new(
        TOOL_SEARCH,
        format!(
            "Discover tools that are available but not listed above. {deferred_count} additional \
             tools (for example large MCP tool collections) are kept out of the prompt to save \
             context. Search them by keyword over their names and descriptions; pass no `query` \
             to list them all (capped by `limit`). Then call `tool_describe` to load a tool's \
             full input schema before invoking it by name."
        ),
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_SEARCH_LIMIT }
            }
        }),
    )
}

/// Build the model-facing spec for the `tool_describe` disclosure tool.
fn tool_describe_spec() -> ToolSpec {
    ToolSpec::new(
        TOOL_DESCRIBE,
        "Return the full description and JSON input schema for one or more tools by name \
         (typically tools found via `tool_search`). Call this before invoking a previously \
         hidden tool so you know its parameters.",
        json!({
            "type": "object",
            "required": ["names"],
            "properties": {
                "names": { "type": "array", "items": { "type": "string" } }
            }
        }),
    )
}
