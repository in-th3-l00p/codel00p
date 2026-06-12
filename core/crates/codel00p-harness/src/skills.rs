//! Skill context injection for a turn.
//!
//! A [`SkillProvider`] selects skills relevant to the current turn; the harness
//! injects them into the inference request as a system message, the same way it
//! injects reviewed project memory. The harness stays decoupled from how skills
//! are stored: it only sees [`SkillPrompt`]s (a name + instructions).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    errors::HarnessError,
    session::{SessionId, TurnId},
};

/// A single skill selected for the current turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPrompt {
    name: String,
    instructions: String,
}

impl SkillPrompt {
    pub fn new(name: impl Into<String>, instructions: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instructions: instructions.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }
}

/// The skills selected for a turn, in priority order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillContext {
    skills: Vec<SkillPrompt>,
}

impl SkillContext {
    pub fn new(skills: Vec<SkillPrompt>) -> Self {
        Self { skills }
    }

    pub fn skills(&self) -> &[SkillPrompt] {
        &self.skills
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

/// What a [`SkillProvider`] needs to pick relevant skills: turn identity plus the
/// user's request text to match against skill triggers.
#[derive(Clone, Debug)]
pub struct SkillSelectionRequest {
    session_id: SessionId,
    turn_id: TurnId,
    message_count: usize,
    query: String,
}

impl SkillSelectionRequest {
    pub fn new(
        session_id: SessionId,
        turn_id: TurnId,
        message_count: usize,
        query: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            turn_id,
            message_count,
            query: query.into(),
        }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }

    pub fn query(&self) -> &str {
        &self.query
    }
}

/// Selects skills relevant to a turn. Implementations own loading and ranking.
#[async_trait]
pub trait SkillProvider: Send + Sync {
    async fn select(&self, request: SkillSelectionRequest) -> Result<SkillContext, HarnessError>;
}

/// Renders a [`SkillContext`] into a system prompt, or `None` when empty.
pub struct SkillPromptAssembler;

impl SkillPromptAssembler {
    pub fn assemble(&self, context: &SkillContext) -> Option<String> {
        if context.is_empty() {
            return None;
        }
        let mut prompt =
            String::from("Relevant skills (procedural knowledge to apply when useful):\n");
        for skill in context.skills() {
            prompt.push_str("\n## Skill: ");
            prompt.push_str(skill.name());
            prompt.push('\n');
            prompt.push_str(skill.instructions());
            prompt.push('\n');
        }
        Some(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_context_assembles_to_nothing() {
        assert!(
            SkillPromptAssembler
                .assemble(&SkillContext::default())
                .is_none()
        );
    }

    #[test]
    fn assembles_selected_skills_into_a_prompt() {
        let context = SkillContext::new(vec![
            SkillPrompt::new("deploy", "Run the deploy steps."),
            SkillPrompt::new("test", "Run the tests."),
        ]);
        let prompt = SkillPromptAssembler.assemble(&context).expect("prompt");
        assert!(prompt.contains("## Skill: deploy"));
        assert!(prompt.contains("Run the deploy steps."));
        assert!(prompt.contains("## Skill: test"));
    }
}
