//! Self-improvement: the agent proposing skills it learned, for human review.
//!
//! After doing work, an agent can call `propose_skill` to capture a reusable
//! procedure. The proposal goes to a [`SkillProposalSink`] as a *candidate* —
//! it never becomes active (and so never reaches a future turn's context) until
//! a human approves it. This is the codel00p version of "skills the agent
//! creates and reuses": the loop proposes, review approves.
//!
//! The harness stays decoupled from skill storage: it only produces a
//! [`ProposedSkill`]; the application persists it.

use std::sync::Arc;

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    session::{SessionId, TurnId},
    tool_registry::ToolRegistry,
    tool_result::ToolResult,
    tools::{Tool, optional_string, required_string},
    workspace::Workspace,
};

/// A reusable procedure the agent proposes after doing work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedSkill {
    name: String,
    description: String,
    triggers: Vec<String>,
    instructions: String,
}

impl ProposedSkill {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        triggers: Vec<String>,
        instructions: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            triggers,
            instructions: instructions.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn triggers(&self) -> &[String] {
        &self.triggers
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }
}

/// Persists a proposed skill as a review candidate. Implementations own storage.
#[async_trait]
pub trait SkillProposalSink: Send + Sync {
    async fn propose(&self, skill: ProposedSkill) -> Result<(), HarnessError>;
}

/// Tool that lets the agent propose a skill learned from the current task.
pub struct ProposeSkillTool {
    sink: Arc<dyn SkillProposalSink>,
}

impl ProposeSkillTool {
    pub fn new(sink: Arc<dyn SkillProposalSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl Tool for ProposeSkillTool {
    fn name(&self) -> &str {
        "propose_skill"
    }

    fn description(&self) -> &str {
        "Propose a reusable skill (a procedure you just learned) for human review. \
         It is recorded as a candidate and is NOT used until a human approves it. \
         Provide a short kebab-case name, a one-line description, optional trigger \
         keywords, and step-by-step instructions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["name", "instructions"],
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string" },
                "triggers": { "type": "array", "items": { "type": "string" } },
                "instructions": { "type": "string" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        // Recording a proposal for review is a meta action gated like a
        // connector; it does not touch the workspace.
        PermissionScope::ExternalConnector
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let name = required_string(self.name(), &input, "name")?;
        let instructions = required_string(self.name(), &input, "instructions")?;
        let description = optional_string(&input, "description").unwrap_or("");
        let triggers = input
            .get("triggers")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let proposal = ProposedSkill::new(name, description, triggers, instructions);
        let proposed_name = proposal.name().to_string();
        self.sink.propose(proposal).await?;

        Ok(ToolResult::json(json!({
            "status": "proposed",
            "name": proposed_name,
            "note": "recorded as a candidate; awaiting human review",
        })))
    }
}

/// Build the learning tools backed by `sink`. Registering these makes an agent
/// able to propose skills for review.
pub fn learning_tools(sink: Arc<dyn SkillProposalSink>) -> ToolRegistry {
    ToolRegistry::new().with_tool_arc(Arc::new(ProposeSkillTool::new(sink)))
}

/// What a [`SkillExtractor`] sees at turn end to decide whether the work yielded
/// a reusable procedure worth proposing.
#[derive(Clone, Debug)]
pub struct SkillExtractionRequest {
    session_id: SessionId,
    turn_id: TurnId,
    goal: String,
    assistant_message: Option<String>,
    tool_calls: Vec<String>,
}

impl SkillExtractionRequest {
    pub fn new(
        session_id: SessionId,
        turn_id: TurnId,
        goal: impl Into<String>,
        assistant_message: Option<String>,
        tool_calls: Vec<String>,
    ) -> Self {
        Self {
            session_id,
            turn_id,
            goal: goal.into(),
            assistant_message,
            tool_calls,
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    /// The user's request that drove the turn.
    pub fn goal(&self) -> &str {
        &self.goal
    }

    pub fn assistant_message(&self) -> Option<&str> {
        self.assistant_message.as_deref()
    }

    /// Names of the tools executed this turn, in order.
    pub fn tool_calls(&self) -> &[String] {
        &self.tool_calls
    }
}

/// Proposes skills automatically from a completed turn. Implementations decide
/// whether the work was skill-worthy; returning an empty vec proposes nothing.
#[async_trait]
pub trait SkillExtractor: Send + Sync {
    async fn extract(
        &self,
        request: SkillExtractionRequest,
    ) -> Result<Vec<ProposedSkill>, HarnessError>;
}

/// Default extractor: proposes a single draft skill when a turn carried out a
/// real procedure — at least `min_steps` workspace-mutating or command tool
/// calls — and ended with an answer. The draft captures the goal and the tool
/// sequence for a human to refine and approve; it is deterministic and makes no
/// extra inference call.
#[derive(Clone, Copy, Debug)]
pub struct ProcedureSkillExtractor {
    min_steps: usize,
}

impl ProcedureSkillExtractor {
    pub fn new(min_steps: usize) -> Self {
        Self {
            min_steps: min_steps.max(1),
        }
    }
}

impl Default for ProcedureSkillExtractor {
    fn default() -> Self {
        Self::new(2)
    }
}

#[async_trait]
impl SkillExtractor for ProcedureSkillExtractor {
    async fn extract(
        &self,
        request: SkillExtractionRequest,
    ) -> Result<Vec<ProposedSkill>, HarnessError> {
        // Only propose from turns that actually finished with an answer.
        if request
            .assistant_message()
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .is_none()
        {
            return Ok(Vec::new());
        }

        let steps: Vec<&str> = request
            .tool_calls()
            .iter()
            .map(String::as_str)
            .filter(|name| is_procedural_tool(name))
            .collect();
        if steps.len() < self.min_steps {
            return Ok(Vec::new());
        }

        let name = slug(request.goal());
        if name.is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![ProposedSkill::new(
            name,
            first_line(request.goal()),
            keywords(request.goal()),
            render_procedure(request.goal(), &steps),
        )])
    }
}

/// Whether a tool name represents a workspace-changing step (vs. read-only
/// inspection), so the extractor only fires on turns that did real work.
fn is_procedural_tool(name: &str) -> bool {
    matches!(
        name,
        "create_file"
            | "update_file"
            | "delete_file"
            | "apply_patch"
            | "run_command"
            | "git_commit"
    )
}

fn slug(goal: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in goal.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

fn first_line(goal: &str) -> String {
    let line = goal.trim().lines().next().unwrap_or("").trim();
    if line.len() > 120 {
        format!("{}...", &line[..117])
    } else {
        line.to_string()
    }
}

fn keywords(goal: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "this", "that", "from", "into", "your", "please", "make",
        "then", "have", "will", "when",
    ];
    let mut seen: Vec<String> = Vec::new();
    for word in goal
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
    {
        if word.len() >= 4 && !STOP.contains(&word) && !seen.iter().any(|w| w == word) {
            seen.push(word.to_string());
        }
        if seen.len() >= 4 {
            break;
        }
    }
    seen
}

fn render_procedure(goal: &str, steps: &[&str]) -> String {
    let mut out = format!("Goal: {}\n\n", first_line(goal));
    out.push_str("Steps (captured automatically from a completed task — edit before approving):\n");
    for (index, step) in steps.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, step));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingSink {
        proposals: Mutex<Vec<ProposedSkill>>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                proposals: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl SkillProposalSink for RecordingSink {
        async fn propose(&self, skill: ProposedSkill) -> Result<(), HarnessError> {
            self.proposals.lock().expect("lock").push(skill);
            Ok(())
        }
    }

    #[tokio::test]
    async fn propose_skill_tool_records_a_proposal() {
        let sink = Arc::new(RecordingSink::new());
        let registry = learning_tools(sink.clone());
        let workspace = Workspace::new(std::env::temp_dir()).expect("workspace");

        let result = registry
            .execute(
                "propose_skill",
                &workspace,
                json!({
                    "name": "deploy",
                    "description": "Ship the app",
                    "triggers": ["deploy", "release"],
                    "instructions": "1. test\n2. ship"
                }),
            )
            .await
            .expect("execute");

        let proposals = sink.proposals.lock().expect("lock");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].name(), "deploy");
        assert_eq!(
            proposals[0].triggers(),
            ["deploy".to_string(), "release".to_string()]
        );
        assert_eq!(proposals[0].instructions(), "1. test\n2. ship");

        assert_eq!(result.content()["status"], "proposed");
        assert_eq!(result.content()["name"], "deploy");
    }

    #[tokio::test]
    async fn propose_skill_requires_name_and_instructions() {
        let registry = learning_tools(Arc::new(RecordingSink::new()));
        let workspace = Workspace::new(std::env::temp_dir()).expect("workspace");

        let error = registry
            .execute("propose_skill", &workspace, json!({ "name": "x" }))
            .await
            .expect_err("missing instructions should error");
        assert!(matches!(error, HarnessError::InvalidToolInput { .. }));
    }

    fn extraction_request(
        goal: &str,
        assistant: Option<&str>,
        tools: &[&str],
    ) -> SkillExtractionRequest {
        SkillExtractionRequest::new(
            SessionId::from_static("s"),
            TurnId::from_static("t"),
            goal,
            assistant.map(str::to_string),
            tools.iter().map(|t| t.to_string()).collect(),
        )
    }

    #[tokio::test]
    async fn extractor_proposes_a_draft_from_a_real_procedure() {
        let extractor = ProcedureSkillExtractor::default();
        let proposals = extractor
            .extract(extraction_request(
                "Set up the deploy pipeline",
                Some("Done."),
                &["read_file", "create_file", "run_command"],
            ))
            .await
            .expect("extract");

        assert_eq!(proposals.len(), 1);
        let skill = &proposals[0];
        assert_eq!(skill.name(), "set-up-the-deploy-pipeline");
        assert_eq!(skill.description(), "Set up the deploy pipeline");
        // Only the procedural steps are captured, in order.
        assert!(skill.instructions().contains("1. create_file"));
        assert!(skill.instructions().contains("2. run_command"));
        assert!(!skill.instructions().contains("read_file"));
        assert!(skill.triggers().iter().any(|t| t == "deploy"));
    }

    #[tokio::test]
    async fn extractor_skips_thin_or_unfinished_turns() {
        let extractor = ProcedureSkillExtractor::default();

        // Too few procedural steps.
        assert!(
            extractor
                .extract(extraction_request(
                    "Read a file",
                    Some("ok"),
                    &["read_file", "create_file"]
                ))
                .await
                .unwrap()
                .is_empty()
        );

        // No final answer.
        assert!(
            extractor
                .extract(extraction_request(
                    "Do stuff",
                    None,
                    &["create_file", "run_command"]
                ))
                .await
                .unwrap()
                .is_empty()
        );
    }
}
