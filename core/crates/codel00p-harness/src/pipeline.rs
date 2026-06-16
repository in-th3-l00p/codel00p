//! Programmatic tool calling: the `run_pipeline` tool.
//!
//! This collapses a multi-step tool pipeline into a single inference. Instead of
//! one model round-trip per tool call, the model declares an ordered list of
//! steps — each a tool name plus its input — and the harness runs them in one
//! shot, passing earlier steps' outputs into later steps via `{{...}}`
//! references.
//!
//! Governance is preserved exactly: **every step is dispatched through the same
//! [`PermissionPolicy`] and the same [`ToolRegistry`] as a direct tool call**.
//! Only *orchestration* moves into the step list, never *authority* — a step the
//! policy denies does not run, and its denial is reported in the result. This is
//! the no-sandbox, fully-governed form of programmatic tool calling; arbitrary
//! in-process code execution waits on an isolating execution backend.

use std::sync::Arc;

use async_trait::async_trait;
use codel00p_protocol::{PermissionScope, RuntimeErrorKind};
use serde_json::{Map, Value, json};

use crate::{
    errors::HarnessError,
    permissions::{PermissionPolicy, PermissionRequest},
    session::{SessionId, TurnId},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

/// Hard ceiling on the number of steps in one pipeline.
const MAX_STEPS: usize = 32;

/// Build a registry fragment exposing `run_pipeline`, backed by `sub_tools`
/// (the tool surface the pipeline may call) and `policy` (the per-step gate).
///
/// `sub_tools` should not itself contain `run_pipeline`, so a pipeline cannot
/// recursively invoke another pipeline.
pub fn pipeline_tools(sub_tools: ToolRegistry, policy: Arc<dyn PermissionPolicy>) -> ToolRegistry {
    ToolRegistry::new().with_tool(RunPipelineTool::new(Arc::new(sub_tools), policy))
}

/// Programmatic tool calling: run a declared sequence of governed tool calls in
/// one inference, passing earlier results into later steps.
pub struct RunPipelineTool {
    sub_tools: Arc<ToolRegistry>,
    policy: Arc<dyn PermissionPolicy>,
}

impl RunPipelineTool {
    pub fn new(sub_tools: Arc<ToolRegistry>, policy: Arc<dyn PermissionPolicy>) -> Self {
        Self { sub_tools, policy }
    }

    /// Parse and validate the `steps` array out of the tool input.
    fn parse_steps(&self, input: &Value) -> Result<Vec<Step>, HarnessError> {
        let steps = input
            .get("steps")
            .and_then(Value::as_array)
            .ok_or_else(|| self.invalid("missing array field `steps`"))?;
        if steps.is_empty() {
            return Err(self.invalid("`steps` must not be empty"));
        }
        if steps.len() > MAX_STEPS {
            return Err(self.invalid(format!(
                "pipeline has {} steps; the maximum is {MAX_STEPS}",
                steps.len()
            )));
        }

        steps
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let tool = required_string(self.name(), value, "tool")?.to_string();
                let id = value.get("id").and_then(Value::as_str).map(str::to_string);
                let input = value.get("input").cloned().unwrap_or_else(|| json!({}));
                if !input.is_object() {
                    return Err(self.invalid(format!("step {index}: `input` must be an object")));
                }
                Ok(Step { id, tool, input })
            })
            .collect()
    }

    fn invalid(&self, message: impl Into<String>) -> HarnessError {
        HarnessError::InvalidToolInput {
            name: self.name().to_string(),
            message: message.into(),
        }
    }
}

struct Step {
    id: Option<String>,
    tool: String,
    input: Value,
}

#[async_trait]
impl Tool for RunPipelineTool {
    fn name(&self) -> &str {
        "run_pipeline"
    }

    fn description(&self) -> &str {
        "Run several tool calls as one governed pipeline in a single step, instead \
         of one model round-trip per call. `steps` is an ordered list of \
         { tool, input, id? }. A later step can reference an earlier step's output \
         with a `{{...}}` template inside any string input value — by index \
         (`{{steps.0.content}}`) or by a step's `id` (`{{readme.content}}`). A \
         template that is the entire string resolves to the referenced JSON value \
         (so you can forward arrays/objects); embedded templates are stringified. \
         Every step is permission-checked and dispatched exactly like a direct \
         tool call. By default the pipeline stops at the first failing step; set \
         `stop_on_error` to false to run the rest anyway."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["steps"],
            "properties": {
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["tool"],
                        "properties": {
                            "tool": { "type": "string" },
                            "input": { "type": "object" },
                            "id": { "type": "string" }
                        }
                    }
                },
                "stop_on_error": { "type": "boolean" }
            }
        })
    }

    /// The pipeline's own scope is the highest scope among its declared steps, so
    /// the outer permission gate is never weaker than what the pipeline can do.
    /// Each step is *also* gated individually at execution time.
    fn permission_scope(&self, input: &Value) -> PermissionScope {
        let Ok(steps) = self.parse_steps(input) else {
            return PermissionScope::ReadOnly;
        };
        steps
            .iter()
            .map(|step| self.sub_tools.permission_scope(&step.tool, &step.input))
            .max_by_key(|scope| scope_rank(*scope))
            .unwrap_or(PermissionScope::ReadOnly)
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let steps = self.parse_steps(&input)?;
        let stop_on_error = input
            .get("stop_on_error")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // Accumulated context for `{{...}}` references: outputs by index under
        // `steps`, and by `id` at the top level.
        let mut context = Map::new();
        context.insert("steps".to_string(), Value::Array(Vec::new()));

        let mut step_reports = Vec::new();
        let mut completed = 0usize;
        let mut stopped_early = false;

        for (index, step) in steps.iter().enumerate() {
            let resolved_input = resolve_references(&step.input, &context);

            let scope = self.sub_tools.permission_scope(&step.tool, &resolved_input);
            let request = PermissionRequest::new(
                format!("pipeline-step-{index}"),
                SessionId::from_static("pipeline"),
                TurnId::from_static("pipeline"),
                &step.tool,
                resolved_input.clone(),
                scope,
            );
            let decision = self.policy.decide(request).await?;

            let (ok, output, error) = if !decision.allows_execution() {
                let message = decision
                    .message()
                    .unwrap_or("tool execution denied by permission policy")
                    .to_string();
                (
                    false,
                    None,
                    Some(json!({
                        "error": message,
                        "error_kind": RuntimeErrorKind::PermissionDenied,
                    })),
                )
            } else {
                match self
                    .sub_tools
                    .execute(&step.tool, workspace, resolved_input.clone())
                    .await
                {
                    Ok(result) => (true, Some(result.content().clone()), None),
                    Err(error) => (false, None, Some(json!({ "error": error.to_string() }))),
                }
            };

            // Record this step's output into the reference context (null on
            // failure so later references resolve predictably).
            let context_value = output.clone().unwrap_or(Value::Null);
            if let Some(Value::Array(array)) = context.get_mut("steps") {
                array.push(context_value.clone());
            }
            if let Some(id) = &step.id {
                context.insert(id.clone(), context_value);
            }

            let mut report = Map::new();
            report.insert("index".to_string(), json!(index));
            if let Some(id) = &step.id {
                report.insert("id".to_string(), json!(id));
            }
            report.insert("tool".to_string(), json!(step.tool));
            report.insert("scope".to_string(), json!(scope_label(scope)));
            report.insert("ok".to_string(), json!(ok));
            if let Some(output) = output {
                report.insert("output".to_string(), output);
            }
            if let Some(error) = error {
                report.insert("error".to_string(), error);
            }
            step_reports.push(Value::Object(report));

            if ok {
                completed += 1;
            } else if stop_on_error {
                stopped_early = true;
                break;
            }
        }

        Ok(ToolResult::json(json!({
            "steps": step_reports,
            "completed": completed,
            "total": steps.len(),
            "stopped_early": stopped_early,
        })))
    }
}

/// Severity ranking used to pick a pipeline's effective permission scope.
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

fn scope_label(scope: PermissionScope) -> &'static str {
    match scope {
        PermissionScope::ReadOnly => "read_only",
        PermissionScope::MemoryWrite => "memory_write",
        PermissionScope::WorkspaceWrite => "workspace_write",
        PermissionScope::Network => "network",
        PermissionScope::Shell => "shell",
        PermissionScope::ExternalConnector => "external_connector",
    }
}

/// Recursively resolve `{{...}}` references inside a JSON value against the
/// accumulated step context. Only string values carry templates; objects and
/// arrays are walked.
fn resolve_references(value: &Value, context: &Map<String, Value>) -> Value {
    match value {
        Value::String(text) => resolve_string(text, context),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| resolve_references(v, context))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), resolve_references(v, context)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Resolve a single string. If the entire string is one `{{expr}}`, the
/// referenced JSON value is returned as-is (preserving its type). Otherwise each
/// `{{expr}}` is replaced by its stringified value and the surrounding text is
/// kept.
fn resolve_string(text: &str, context: &Map<String, Value>) -> Value {
    let trimmed = text.trim();
    if let Some(expr) = whole_template(trimmed) {
        return lookup(expr.trim(), context).unwrap_or(Value::Null);
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // No closing brace; emit the rest verbatim.
            out.push_str(&rest[start..]);
            return Value::String(out);
        };
        let expr = after[..end].trim();
        match lookup(expr, context) {
            Some(Value::String(s)) => out.push_str(&s),
            Some(other) => out.push_str(&other.to_string()),
            None => {} // Unresolved reference collapses to empty.
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Value::String(out)
}

/// If `text` is exactly one `{{...}}` template, return the inner expression.
fn whole_template(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("{{")?.strip_suffix("}}")?;
    // Reject strings with more than one template (e.g. `{{a}}{{b}}`).
    if inner.contains("{{") || inner.contains("}}") {
        None
    } else {
        Some(inner)
    }
}

/// Look up a dotted path (with `[n]` or `.n` indices) in the context.
fn lookup(expr: &str, context: &Map<String, Value>) -> Option<Value> {
    let normalized = expr.replace('[', ".").replace(']', "");
    let mut segments = normalized.split('.').filter(|segment| !segment.is_empty());
    let mut cursor = context.get(segments.next()?)?.clone();
    for segment in segments {
        cursor = match cursor {
            Value::Object(map) => map.get(segment)?.clone(),
            Value::Array(array) => array.get(segment.parse::<usize>().ok()?)?.clone(),
            _ => return None,
        };
    }
    Some(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{PermissionDecision, PermissionMode};
    use async_trait::async_trait;

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

    fn tool(policy: Arc<dyn PermissionPolicy>) -> RunPipelineTool {
        let sub =
            ToolRegistry::read_only_defaults().with_registry(ToolRegistry::editing_defaults());
        RunPipelineTool::new(Arc::new(sub), policy)
    }

    #[tokio::test]
    async fn runs_steps_in_order_and_reports() {
        let (_dir, ws) = workspace_with(&[("a.txt", "hello\nworld\n")]);
        let pipe = tool(Arc::new(crate::permissions::AllowAllPermissionPolicy));

        let result = pipe
            .execute(
                &ws,
                json!({
                    "steps": [
                        { "tool": "read_file", "input": { "path": "a.txt" } },
                        { "tool": "list_files", "input": {} }
                    ]
                }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["completed"], 2);
        assert_eq!(content["total"], 2);
        assert_eq!(content["stopped_early"], false);
        let steps = content["steps"].as_array().unwrap();
        assert_eq!(steps[0]["tool"], "read_file");
        assert_eq!(steps[0]["ok"], true);
    }

    #[tokio::test]
    async fn passes_output_between_steps() {
        // Read a path from one file, then create a new file whose content embeds it.
        let (_dir, ws) = workspace_with(&[("src.txt", "PAYLOAD")]);
        let pipe = tool(Arc::new(crate::permissions::AllowAllPermissionPolicy));

        let result = pipe
            .execute(
                &ws,
                json!({
                    "steps": [
                        { "tool": "read_file", "id": "src", "input": { "path": "src.txt" } },
                        {
                            "tool": "create_file",
                            "input": { "path": "out.txt", "content": "{{src.content}}" }
                        },
                        { "tool": "read_file", "input": { "path": "out.txt" } }
                    ]
                }),
            )
            .await
            .unwrap();

        let steps = result.content()["steps"].as_array().unwrap();
        // The final read sees the content forwarded from step 0.
        assert_eq!(steps[2]["output"]["content"], "PAYLOAD");
    }

    #[tokio::test]
    async fn stops_on_first_error_by_default() {
        let (_dir, ws) = workspace_with(&[]);
        let pipe = tool(Arc::new(crate::permissions::AllowAllPermissionPolicy));

        let result = pipe
            .execute(
                &ws,
                json!({
                    "steps": [
                        { "tool": "read_file", "input": { "path": "missing.txt" } },
                        { "tool": "list_files", "input": {} }
                    ]
                }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["completed"], 0);
        assert_eq!(content["stopped_early"], true);
        // The second step never ran.
        assert_eq!(content["steps"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn continue_on_error_runs_remaining_steps() {
        let (_dir, ws) = workspace_with(&[("a.txt", "x")]);
        let pipe = tool(Arc::new(crate::permissions::AllowAllPermissionPolicy));

        let result = pipe
            .execute(
                &ws,
                json!({
                    "stop_on_error": false,
                    "steps": [
                        { "tool": "read_file", "input": { "path": "missing.txt" } },
                        { "tool": "read_file", "input": { "path": "a.txt" } }
                    ]
                }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["completed"], 1);
        assert_eq!(content["steps"].as_array().unwrap().len(), 2);
        assert_eq!(content["steps"][1]["ok"], true);
    }

    #[tokio::test]
    async fn denied_step_is_not_executed() {
        let (_dir, ws) = workspace_with(&[("a.txt", "x")]);
        let pipe = tool(Arc::new(DenyList {
            deny: vec!["create_file"],
        }));

        let result = pipe
            .execute(
                &ws,
                json!({
                    "stop_on_error": false,
                    "steps": [
                        { "tool": "create_file", "input": { "path": "new.txt", "content": "y" } },
                        { "tool": "read_file", "input": { "path": "a.txt" } }
                    ]
                }),
            )
            .await
            .unwrap();

        let steps = result.content()["steps"].as_array().unwrap();
        assert_eq!(steps[0]["ok"], false);
        assert_eq!(steps[0]["error"]["error_kind"], "permission_denied");
        // The denied file was not created.
        assert!(!_dir.path().join("new.txt").exists());
    }

    #[tokio::test]
    async fn pipeline_scope_is_max_of_steps() {
        let pipe = tool(Arc::new(crate::permissions::AllowAllPermissionPolicy));
        let input = json!({
            "steps": [
                { "tool": "read_file", "input": { "path": "a" } },
                { "tool": "create_file", "input": { "path": "b", "content": "c" } }
            ]
        });
        // read_file is ReadOnly, create_file is WorkspaceWrite → max is WorkspaceWrite.
        assert_eq!(
            pipe.permission_scope(&input),
            PermissionScope::WorkspaceWrite
        );
    }

    #[test]
    fn whole_template_detection() {
        assert_eq!(whole_template("{{a.b}}"), Some("a.b"));
        assert_eq!(whole_template("x {{a}}"), None);
        assert_eq!(whole_template("{{a}}{{b}}"), None);
    }

    #[test]
    fn lookup_resolves_paths_and_indices() {
        let mut ctx = Map::new();
        ctx.insert(
            "steps".to_string(),
            json!([{ "content": "hi" }, { "matches": [{ "path": "p" }] }]),
        );
        assert_eq!(lookup("steps.0.content", &ctx), Some(json!("hi")));
        assert_eq!(lookup("steps[1].matches[0].path", &ctx), Some(json!("p")));
        assert_eq!(lookup("steps.5.x", &ctx), None);
    }
}
