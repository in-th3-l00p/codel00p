//! Capability synthesis: freeze a successful governed pipeline into a named,
//! parameterized, reviewable tool the agent can call in one shot.
//!
//! This is the first slice of the capability flywheel. A [`Capability`] is a
//! frozen [`run_pipeline`](crate::pipeline) program with named parameters: the
//! step inputs reference `{{params.<name>}}`, and calling the capability seeds
//! those params and runs the pipeline through the shared [`PipelineEngine`]. So a
//! synthesized capability is *executable, parameterized, and minimally scoped*
//! (its permission scope is the max of its steps), and — crucially — every step
//! still goes through the registry and permission policy. Authority never moves
//! into a capability; only a reusable orchestration does.
//!
//! `propose_capability` lets the agent (or an extractor) submit a candidate to a
//! [`CapabilityProposalSink`] — a review queue — rather than registering it
//! directly, matching codel00p's reviewed-memory governance. Approved
//! capabilities are loaded with [`load_capabilities`] and registered as tools
//! with [`capability_tools`].

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    errors::HarnessError,
    permissions::PermissionPolicy,
    pipeline::{PipelineEngine, PipelineStep, max_step_scope, parse_steps, scope_label},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    workspace::Workspace,
};

/// A frozen, parameterized pipeline that the agent can call as a single tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    /// The tool name the capability is exposed under.
    pub name: String,
    /// One-line description shown to the model.
    pub description: String,
    /// JSON Schema for the capability's parameters (the tool's `input_schema`).
    #[serde(default = "empty_object_schema")]
    pub parameters: Value,
    /// The frozen pipeline steps, each `{ tool, input, id? }`, where `input`
    /// values may reference `{{params.<name>}}` and `{{steps.N.field}}`.
    pub steps: Vec<Value>,
}

fn empty_object_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

impl Capability {
    /// Validate the capability's shape: a legal tool name, an object parameter
    /// schema, and a well-formed, non-empty step list.
    pub fn validate(&self) -> Result<Vec<PipelineStep>, HarnessError> {
        if !is_valid_tool_name(&self.name) {
            return Err(self.invalid(format!(
                "`{}` is not a valid capability name (use lowercase letters, digits, \
                 and underscores; start with a letter)",
                self.name
            )));
        }
        if self.description.trim().is_empty() {
            return Err(self.invalid("`description` must not be empty".to_string()));
        }
        if !self.parameters.is_object() {
            return Err(self.invalid("`parameters` must be a JSON Schema object".to_string()));
        }
        parse_steps(&self.name, &self.steps)
    }

    fn invalid(&self, message: String) -> HarnessError {
        HarnessError::InvalidToolInput {
            name: "propose_capability".to_string(),
            message,
        }
    }
}

/// A legal tool name: starts with a lowercase letter, then lowercase letters,
/// digits, or underscores.
fn is_valid_tool_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && name.len() <= 64
}

/// A synthesized capability exposed as a callable tool.
pub struct CapabilityTool {
    capability: Capability,
    steps: Vec<PipelineStep>,
    engine: PipelineEngine,
}

impl CapabilityTool {
    /// Build a tool from a capability, validating its steps against `engine`'s
    /// tool surface.
    pub fn new(capability: Capability, engine: PipelineEngine) -> Result<Self, HarnessError> {
        let steps = capability.validate()?;
        Ok(Self {
            capability,
            steps,
            engine,
        })
    }
}

#[async_trait]
impl Tool for CapabilityTool {
    fn name(&self) -> &str {
        &self.capability.name
    }

    fn description(&self) -> &str {
        &self.capability.description
    }

    fn input_schema(&self) -> Value {
        self.capability.parameters.clone()
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        self.engine.max_scope(&self.steps)
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        // The call's arguments become `{{params.<name>}}` for the frozen steps.
        let mut context = Map::new();
        context.insert("params".to_string(), input);

        let run = self
            .engine
            .run(
                workspace,
                &self.steps,
                true,
                &format!("capability-{}", self.capability.name),
                context,
            )
            .await?;

        let ok = run.completed == run.total && !run.stopped_early;
        Ok(ToolResult::json(json!({
            "capability": self.capability.name,
            "ok": ok,
            "steps": run.reports,
            "completed": run.completed,
            "total": run.total,
        })))
    }
}

/// Register a set of approved capabilities as callable tools backed by `engine`
/// over `sub_tools` (the surface the capabilities may call) and `policy`.
pub fn capability_tools(
    sub_tools: ToolRegistry,
    policy: Arc<dyn PermissionPolicy>,
    capabilities: Vec<Capability>,
) -> Result<ToolRegistry, HarnessError> {
    let engine = PipelineEngine::new(Arc::new(sub_tools), policy);
    let mut registry = ToolRegistry::new();
    for capability in capabilities {
        let tool = CapabilityTool::new(capability, engine.clone())?;
        registry = registry.with_tool_arc(Arc::new(tool));
    }
    Ok(registry)
}

/// Load approved capabilities from a directory of `<name>.json` files. Files
/// that are not valid capability JSON are skipped.
pub fn load_capabilities(dir: impl AsRef<Path>) -> Vec<Capability> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut capabilities = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(capability) = serde_json::from_str::<Capability>(&contents)
            && capability.validate().is_ok()
        {
            capabilities.push(capability);
        }
    }
    capabilities.sort_by(|a, b| a.name.cmp(&b.name));
    capabilities
}

/// A destination for proposed capabilities awaiting review/approval.
#[async_trait]
pub trait CapabilityProposalSink: Send + Sync {
    async fn propose(&self, capability: Capability) -> Result<(), HarnessError>;
}

/// A sink that writes each proposed capability to `<dir>/<name>.json` for review.
pub struct FileCapabilityProposalSink {
    dir: std::path::PathBuf,
}

impl FileCapabilityProposalSink {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

#[async_trait]
impl CapabilityProposalSink for FileCapabilityProposalSink {
    async fn propose(&self, capability: Capability) -> Result<(), HarnessError> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{}.json", capability.name));
        let json = serde_json::to_string_pretty(&capability).map_err(|error| {
            HarnessError::ToolFailed {
                name: "propose_capability".to_string(),
                message: error.to_string(),
            }
        })?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Lets the agent propose freezing a pipeline into a reusable capability. The
/// proposal is validated and sent to a review sink — it is **not** registered or
/// executed here.
pub struct ProposeCapabilityTool {
    sub_tools: Arc<ToolRegistry>,
    sink: Arc<dyn CapabilityProposalSink>,
}

impl ProposeCapabilityTool {
    pub fn new(sub_tools: Arc<ToolRegistry>, sink: Arc<dyn CapabilityProposalSink>) -> Self {
        Self { sub_tools, sink }
    }
}

#[async_trait]
impl Tool for ProposeCapabilityTool {
    fn name(&self) -> &str {
        "propose_capability"
    }

    fn description(&self) -> &str {
        "Propose freezing a multi-step tool pipeline you found useful into a named, \
         reusable capability (a tool) for future tasks. Provide a `name`, a \
         `description`, a `parameters` JSON Schema for its inputs, and the `steps` \
         (same shape as run_pipeline; reference inputs with `{{params.<name>}}` and \
         earlier outputs with `{{steps.N.field}}`). The proposal is queued for \
         review, not executed — once approved it becomes a callable tool. Propose a \
         capability when a sequence is worth repeating, not for one-off work."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["name", "description", "steps"],
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string" },
                "parameters": { "type": "object" },
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
                }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        // Proposing only queues a candidate; it executes nothing.
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let name = required_string(self.name(), &input, "name")?.to_string();
        let description = required_string(self.name(), &input, "description")?.to_string();
        let parameters = input
            .get("parameters")
            .cloned()
            .unwrap_or_else(empty_object_schema);
        let steps = input
            .get("steps")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "missing array field `steps`".to_string(),
            })?;

        let capability = Capability {
            name,
            description,
            parameters,
            steps,
        };
        // Validate shape and resolve the steps so we can report the inferred scope.
        let parsed = capability.validate()?;
        let scope = max_step_scope(&self.sub_tools, &parsed);

        self.sink.propose(capability.clone()).await?;

        Ok(ToolResult::json(json!({
            "proposed": true,
            "name": capability.name,
            "step_count": parsed.len(),
            "inferred_scope": scope_label(scope),
            "status": "pending_review",
            "note": optional_string(&input, "note"),
        })))
    }
}

mod extract;
pub use extract::{
    CapabilityCandidateCall, CapabilityExtractionRequest, CapabilityExtractor,
    ModelCapabilityExtractor, PipelineCapabilityExtractor,
};
// Only the test module replays a parsed capability directly.
#[cfg(test)]
use extract::parse_capability_json;

// ===========================================================================
// Verification gate: replay a capability on a throwaway workspace before trust.
// ===========================================================================

/// The result of verifying a capability by replaying it.
#[derive(Clone, Debug)]
pub struct VerificationOutcome {
    pub ok: bool,
    pub completed: usize,
    pub total: usize,
    pub report: Value,
}

/// Smoke-test a capability by running it once on a **fresh temporary workspace**
/// with `sample_params`, through `sub_tools` + `policy`. A capability that
/// completes every step verifies; one whose steps error or are denied does not.
///
/// This is a promotion gate: it proves a synthesized capability actually runs
/// before it is trusted, rather than merely being well-formed. It runs on an
/// empty workspace, so it best fits self-contained (e.g. scaffolding)
/// capabilities; capabilities that depend on pre-existing files should be
/// verified against a seeded fixture by the caller.
pub async fn verify_capability(
    capability: &Capability,
    sub_tools: ToolRegistry,
    policy: Arc<dyn PermissionPolicy>,
    sample_params: Value,
) -> Result<VerificationOutcome, HarnessError> {
    let dir = tempfile::tempdir().map_err(HarnessError::from)?;
    let workspace = Workspace::new(dir.path())?;
    let engine = PipelineEngine::new(Arc::new(sub_tools), policy);
    let tool = CapabilityTool::new(capability.clone(), engine)?;
    let result = tool.execute(&workspace, sample_params).await?;
    let content = result.content();
    let completed = content["completed"].as_u64().unwrap_or(0) as usize;
    let total = content["total"].as_u64().unwrap_or(0) as usize;
    Ok(VerificationOutcome {
        ok: content["ok"].as_bool().unwrap_or(false),
        completed,
        total,
        report: content.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{
        AllowAllPermissionPolicy, PermissionDecision, PermissionMode, PermissionRequest,
    };
    use std::sync::Mutex;

    fn engine() -> PipelineEngine {
        let sub =
            ToolRegistry::read_only_defaults().with_registry(ToolRegistry::editing_defaults());
        PipelineEngine::new(Arc::new(sub), Arc::new(AllowAllPermissionPolicy))
    }

    fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            std::fs::write(dir.path().join(path), content).unwrap();
        }
        let ws = Workspace::new(dir.path()).unwrap();
        (dir, ws)
    }

    fn scaffold_capability() -> Capability {
        Capability {
            name: "scaffold_module".to_string(),
            description: "Create a source file and its test file for a module".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["name"],
                "properties": { "name": { "type": "string" } }
            }),
            steps: vec![
                json!({
                    "tool": "create_file",
                    "input": { "path": "src/{{params.name}}.rs", "content": "// {{params.name}}\n" }
                }),
                json!({
                    "tool": "create_file",
                    "input": {
                        "path": "tests/{{params.name}}_test.rs",
                        "content": "// tests for {{params.name}}\n"
                    }
                }),
            ],
        }
    }

    #[tokio::test]
    async fn capability_runs_its_pipeline_with_params() {
        let (dir, ws) = workspace_with(&[]);
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();

        let tool = CapabilityTool::new(scaffold_capability(), engine()).unwrap();
        let result = tool
            .execute(&ws, json!({ "name": "widget" }))
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["ok"], true);
        assert_eq!(content["completed"], 2);
        assert!(dir.path().join("src/widget.rs").exists());
        assert!(dir.path().join("tests/widget_test.rs").exists());
        let body = std::fs::read_to_string(dir.path().join("src/widget.rs")).unwrap();
        assert_eq!(body, "// widget\n");
    }

    #[test]
    fn capability_scope_is_max_of_steps() {
        // create_file is WorkspaceWrite → the capability is WorkspaceWrite.
        let tool = CapabilityTool::new(scaffold_capability(), engine()).unwrap();
        assert_eq!(
            tool.permission_scope(&json!({})),
            PermissionScope::WorkspaceWrite
        );
    }

    #[test]
    fn invalid_capability_name_is_rejected() {
        let mut cap = scaffold_capability();
        cap.name = "Bad Name!".to_string();
        assert!(CapabilityTool::new(cap, engine()).is_err());
    }

    #[test]
    fn capability_with_unknown_tool_validates_but_fails_at_call_time() {
        // parse_steps accepts any tool name; an unknown tool surfaces at run time.
        let mut cap = scaffold_capability();
        cap.steps = vec![json!({ "tool": "no_such_tool", "input": {} })];
        let tool = CapabilityTool::new(cap, engine()).unwrap();
        // The scope of an unknown tool falls back to the registry default.
        let _ = tool.permission_scope(&json!({}));
    }

    /// Records proposals in memory for assertions.
    #[derive(Default)]
    struct RecordingSink {
        proposals: Mutex<Vec<Capability>>,
    }

    #[async_trait]
    impl CapabilityProposalSink for RecordingSink {
        async fn propose(&self, capability: Capability) -> Result<(), HarnessError> {
            self.proposals.lock().unwrap().push(capability);
            Ok(())
        }
    }

    #[tokio::test]
    async fn propose_capability_validates_and_queues() {
        let (_dir, ws) = workspace_with(&[]);
        let sink = Arc::new(RecordingSink::default());
        let sub = Arc::new(
            ToolRegistry::read_only_defaults().with_registry(ToolRegistry::editing_defaults()),
        );
        let tool = ProposeCapabilityTool::new(sub, sink.clone());

        let cap = scaffold_capability();
        let result = tool
            .execute(
                &ws,
                json!({
                    "name": cap.name,
                    "description": cap.description,
                    "parameters": cap.parameters,
                    "steps": cap.steps
                }),
            )
            .await
            .unwrap();

        let content = result.content();
        assert_eq!(content["proposed"], true);
        assert_eq!(content["inferred_scope"], "workspace_write");
        assert_eq!(content["status"], "pending_review");
        assert_eq!(sink.proposals.lock().unwrap().len(), 1);
        assert_eq!(sink.proposals.lock().unwrap()[0].name, "scaffold_module");
    }

    #[tokio::test]
    async fn propose_capability_rejects_bad_name() {
        let (_dir, ws) = workspace_with(&[]);
        let sink = Arc::new(RecordingSink::default());
        let sub = Arc::new(ToolRegistry::read_only_defaults());
        let tool = ProposeCapabilityTool::new(sub, sink.clone());

        let error = tool
            .execute(
                &ws,
                json!({ "name": "Bad!", "description": "x", "steps": [{ "tool": "read_file" }] }),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
        assert!(sink.proposals.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn file_sink_roundtrips_through_load() {
        let dir = tempfile::tempdir().unwrap();
        let sink = FileCapabilityProposalSink::new(dir.path());
        sink.propose(scaffold_capability()).await.unwrap();

        let loaded = load_capabilities(dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], scaffold_capability());
    }

    #[tokio::test]
    async fn capability_tools_registers_callable_tools() {
        let registry = capability_tools(
            ToolRegistry::editing_defaults(),
            Arc::new(AllowAllPermissionPolicy),
            vec![scaffold_capability()],
        )
        .unwrap();
        assert!(registry.names().contains(&"scaffold_module".to_string()));
    }

    /// Denies create_file to prove capability steps stay governed.
    struct DenyCreate;
    #[async_trait]
    impl PermissionPolicy for DenyCreate {
        async fn decide(
            &self,
            request: PermissionRequest,
        ) -> Result<PermissionDecision, HarnessError> {
            if request.tool_name() == "create_file" {
                Ok(PermissionDecision::deny(
                    request.id(),
                    PermissionMode::Deny,
                    "blocked",
                ))
            } else {
                Ok(PermissionDecision::allow(
                    request.id(),
                    PermissionMode::Allow,
                ))
            }
        }
    }

    #[tokio::test]
    async fn capability_steps_are_permission_gated() {
        let (dir, ws) = workspace_with(&[]);
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let sub = ToolRegistry::editing_defaults();
        let engine = PipelineEngine::new(Arc::new(sub), Arc::new(DenyCreate));
        let tool = CapabilityTool::new(scaffold_capability(), engine).unwrap();

        let result = tool
            .execute(&ws, json!({ "name": "widget" }))
            .await
            .unwrap();
        assert_eq!(result.content()["ok"], false);
        // The denied write never happened.
        assert!(!dir.path().join("src/widget.rs").exists());
    }

    // --- auto-extraction ---

    /// A successful run_pipeline call as the extractor would see it.
    fn successful_pipeline_call(steps: Value) -> CapabilityCandidateCall {
        let total = steps.as_array().map(|s| s.len()).unwrap_or(0);
        CapabilityCandidateCall {
            name: "run_pipeline".to_string(),
            input: json!({ "steps": steps }),
            output: json!({ "completed": total, "total": total, "stopped_early": false }),
        }
    }

    #[tokio::test]
    async fn pipeline_extractor_freezes_a_successful_pipeline() {
        let steps = json!([
            { "tool": "create_file", "input": { "path": "src/a.rs", "content": "x" } },
            { "tool": "create_file", "input": { "path": "tests/a.rs", "content": "y" } }
        ]);
        let request = CapabilityExtractionRequest {
            goal: "Scaffold a module".to_string(),
            assistant_message: Some("done".to_string()),
            calls: vec![successful_pipeline_call(steps.clone())],
        };
        let candidate = PipelineCapabilityExtractor
            .extract(request)
            .await
            .unwrap()
            .expect("a candidate");
        assert_eq!(candidate.name, "scaffold_a_module");
        assert_eq!(candidate.steps, steps.as_array().unwrap().clone());
    }

    #[tokio::test]
    async fn pipeline_extractor_skips_when_no_successful_pipeline() {
        // A failed pipeline yields no candidate.
        let request = CapabilityExtractionRequest {
            goal: "Do a thing".to_string(),
            assistant_message: Some("done".to_string()),
            calls: vec![CapabilityCandidateCall {
                name: "run_pipeline".to_string(),
                input: json!({ "steps": [{ "tool": "create_file" }, { "tool": "create_file" }] }),
                output: json!({ "completed": 1, "total": 2, "stopped_early": true }),
            }],
        };
        assert!(
            PipelineCapabilityExtractor
                .extract(request)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn pipeline_extractor_skips_thin_or_unfinished_turns() {
        // No assistant message → not finished.
        let single = json!([{ "tool": "create_file", "input": { "path": "a", "content": "b" } }]);
        let unfinished = CapabilityExtractionRequest {
            goal: "x".to_string(),
            assistant_message: None,
            calls: vec![successful_pipeline_call(single.clone())],
        };
        assert!(
            PipelineCapabilityExtractor
                .extract(unfinished)
                .await
                .unwrap()
                .is_none()
        );
        // Single-step pipeline is not worth freezing.
        let thin = CapabilityExtractionRequest {
            goal: "x".to_string(),
            assistant_message: Some("done".to_string()),
            calls: vec![successful_pipeline_call(single)],
        };
        assert!(
            PipelineCapabilityExtractor
                .extract(thin)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn parse_capability_json_tolerates_fences_and_prose() {
        let reply = "Here you go:\n```json\n{\"name\":\"x\",\"description\":\"d\",\
            \"parameters\":{\"type\":\"object\"},\"steps\":[{\"tool\":\"read_file\"}]}\n```\nDone.";
        let cap = parse_capability_json(reply).expect("parsed");
        assert_eq!(cap.name, "x");
        assert_eq!(cap.steps.len(), 1);
    }

    // --- verification gate ---

    #[tokio::test]
    async fn verify_capability_passes_for_a_runnable_capability() {
        let outcome = verify_capability(
            &scaffold_capability(),
            ToolRegistry::editing_defaults(),
            Arc::new(AllowAllPermissionPolicy),
            json!({ "name": "verified" }),
        )
        .await
        .unwrap();
        assert!(outcome.ok);
        assert_eq!(outcome.completed, 2);
        assert_eq!(outcome.total, 2);
    }

    #[tokio::test]
    async fn verify_capability_fails_when_a_step_is_denied() {
        let outcome = verify_capability(
            &scaffold_capability(),
            ToolRegistry::editing_defaults(),
            Arc::new(DenyCreate),
            json!({ "name": "verified" }),
        )
        .await
        .unwrap();
        assert!(!outcome.ok);
        assert_eq!(outcome.completed, 0);
    }
}
