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
}
