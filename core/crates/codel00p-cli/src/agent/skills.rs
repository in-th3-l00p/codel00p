//! Local skill selection and proposal sinks for agent turns.

use super::*;

/// Default number of skills injected into a turn.
const SKILL_SELECTION_LIMIT: usize = 3;

/// Selects locally-authored skills relevant to the turn and hands them to the
/// harness for injection. Loading is filesystem-only, so it runs inline.
///
/// Each selected skill's usage is recorded once per turn. The provider is built
/// fresh per turn, so `recorded` deduplicates across the agentic loop's
/// iterations (which each call `select`).
pub(super) struct CliSkillProvider {
    sources: Vec<(SkillSource, PathBuf)>,
    limit: usize,
    recorded: Mutex<HashSet<String>>,
}

impl CliSkillProvider {
    pub(super) fn new(sources: Vec<(SkillSource, PathBuf)>) -> Self {
        Self {
            sources,
            limit: SKILL_SELECTION_LIMIT,
            recorded: Mutex::new(HashSet::new()),
        }
    }

    /// True the first time `name` is seen this turn, so usage is counted once.
    fn first_use_this_turn(&self, name: &str) -> bool {
        self.recorded
            .lock()
            .expect("usage lock")
            .insert(name.to_string())
    }
}

#[async_trait]
impl SkillProvider for CliSkillProvider {
    async fn select(&self, request: SkillSelectionRequest) -> Result<SkillContext, HarnessError> {
        let skills = load_skills(&self.sources);
        let selected = select_skills(&skills, request.query(), self.limit);
        let now = now_epoch_secs();

        let prompts = selected
            .into_iter()
            .map(|skill| {
                if self.first_use_this_turn(&skill.name) {
                    // Best-effort: usage tracking must never fail a turn.
                    let _ = record_skill_usage(&skill, now);
                }
                SkillPrompt::new(skill.name, skill.body)
            })
            .collect();
        Ok(SkillContext::new(prompts))
    }
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Records agent-proposed skills as review candidates under the user skills dir.
/// Proposals stay inactive until a human runs `codel00p skills approve`.
pub(super) struct CliSkillProposalSink {
    skills_dir: PathBuf,
}

impl CliSkillProposalSink {
    pub(super) fn new(skills_dir: PathBuf) -> Self {
        Self { skills_dir }
    }
}

#[async_trait]
impl SkillProposalSink for CliSkillProposalSink {
    async fn propose(&self, skill: ProposedSkill) -> Result<(), HarnessError> {
        let proposal = SkillProposal {
            name: skill.name().to_string(),
            description: skill.description().to_string(),
            triggers: skill.triggers().to_vec(),
            instructions: skill.instructions().to_string(),
            created_by: "agent".to_string(),
        };
        match propose_skill(&self.skills_dir, &proposal) {
            // Idempotent: a name already proposed or active is a benign no-op,
            // so repeated tasks (e.g. automatic extraction) stay quiet.
            Ok(_)
            | Err(SkillError::CandidateExists { .. })
            | Err(SkillError::AlreadyActive { .. }) => Ok(()),
            Err(error) => Err(HarnessError::Configuration {
                message: error.to_string(),
            }),
        }
    }
}
