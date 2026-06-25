//! The full-screen `codel00p skills` review dialog.
//!
//! Bare `codel00p skills` opens this on a terminal; the scriptable subcommands
//! (`list`, `show`, `approve`, ...) remain for non-TTY use. The pure model and
//! rendering live in [`model`] and [`view`]; the terminal lifecycle and loop are
//! shared via [`crate::dialog`], and the closure here performs the skill-store
//! effects an update asks for.

mod model;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codel00p_skill::{
    DEFAULT_SKILL_CONSOLIDATION_THRESHOLD, Skill, SkillSource, approve_candidate, archive_skill,
    load_archived, load_candidates, load_skills, load_usage, plan_skill_consolidations,
    reject_candidate, restore_skill,
};

use crate::config::CliResult;
use crate::skills::{curator_enabled, skill_sources, user_skills_dir};
use model::{Flow, Mutation, NearDuplicate, SkillKind, SkillRow, SkillsModel};

/// Runs the review dialog. The terminal lifecycle and blocking loop come from
/// [`crate::dialog`]; the `on_key` closure performs the skill-store effects.
pub(crate) fn run(workspace_start: &Path) -> CliResult<String> {
    let curator_on = curator_enabled(workspace_start);
    let mut model = SkillsModel::new();
    model.set_curator_enabled(curator_on);
    model.set_rows(load_rows(workspace_start, curator_on));

    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Stay | Flow::OpenDetail => {}
            Flow::Quit => return Ok(false),
            Flow::Mutate(mutation) => {
                apply(model, mutation);
                model.screen = model::Screen::List;
                model.selected = None;
                model.set_rows(load_rows(workspace_start, curator_on));
            }
        }
        Ok(true)
    })?;
    Ok("Closed skills review.\n".to_string())
}

/// Builds the combined active-skill + candidate row set for the model. When the
/// curator is enabled, near-duplicate active skills are annotated (via
/// `plan_skill_consolidations`) so the dialog can offer the `c` consolidate action.
fn load_rows(workspace_start: &Path, curator_on: bool) -> Vec<SkillRow> {
    let sources = skill_sources(workspace_start);
    let skills = load_skills(&sources);
    // The skills root each active skill lives under, keyed by source, so a
    // mutation can target the right directory.
    let root_for = |source: SkillSource| -> PathBuf {
        sources
            .iter()
            .find(|(s, _)| *s == source)
            .map(|(_, dir)| dir.clone())
            .unwrap_or_else(user_skills_dir)
    };

    // Curator finding map: duplicate skill name -> survivor + similarity. Built
    // only when the opt-in curator is enabled.
    let near_dups: HashMap<String, NearDuplicate> = if curator_on {
        let usage_for = |skill: &Skill| load_usage(&root_for(skill.source)).get(&skill.name);
        plan_skill_consolidations(&skills, usage_for, DEFAULT_SKILL_CONSOLIDATION_THRESHOLD)
            .into_iter()
            .flat_map(|consolidation| {
                let survivor = consolidation.survivor().name.clone();
                consolidation
                    .duplicates()
                    .iter()
                    .map(|duplicate| {
                        (
                            duplicate.skill().name.clone(),
                            NearDuplicate {
                                survivor: survivor.clone(),
                                similarity: duplicate.similarity(),
                            },
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    } else {
        HashMap::new()
    };

    let mut rows: Vec<SkillRow> = Vec::new();
    for skill in &skills {
        let root = root_for(skill.source);
        let usage = load_usage(&root).get(&skill.name);
        let mut row = active_row(skill, root, usage.count);
        row.near_duplicate_of = near_dups.get(&skill.name).cloned();
        rows.push(row);
    }

    for (_, dir) in &sources {
        for candidate in load_candidates(dir) {
            rows.push(candidate_row(&candidate, dir.clone()));
        }
        for disabled in load_archived(dir) {
            rows.push(disabled_row(&disabled, dir.clone()));
        }
    }

    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

fn active_row(skill: &Skill, root: PathBuf, usage: u64) -> SkillRow {
    SkillRow {
        name: skill.name.clone(),
        kind: SkillKind::Active,
        source: skill.source.label().to_string(),
        created_by: skill.created_by.clone(),
        usage,
        description: skill.description.clone(),
        body: skill.body.clone(),
        version: skill.version.clone(),
        triggers: skill.triggers.clone(),
        path: skill.path.display().to_string(),
        root,
        near_duplicate_of: None,
    }
}

fn candidate_row(skill: &Skill, root: PathBuf) -> SkillRow {
    SkillRow {
        name: skill.name.clone(),
        kind: SkillKind::Candidate,
        source: skill.source.label().to_string(),
        created_by: skill.created_by.clone(),
        usage: 0,
        description: skill.description.clone(),
        body: skill.body.clone(),
        version: skill.version.clone(),
        triggers: skill.triggers.clone(),
        path: skill.path.display().to_string(),
        root,
        near_duplicate_of: None,
    }
}

fn disabled_row(skill: &Skill, root: PathBuf) -> SkillRow {
    SkillRow {
        name: skill.name.clone(),
        kind: SkillKind::Disabled,
        source: skill.source.label().to_string(),
        created_by: skill.created_by.clone(),
        usage: 0,
        description: skill.description.clone(),
        body: skill.body.clone(),
        version: skill.version.clone(),
        triggers: skill.triggers.clone(),
        path: skill.path.display().to_string(),
        root,
        near_duplicate_of: None,
    }
}

/// Applies a review effect, recording success or the error in the status line.
fn apply(model: &mut SkillsModel, mutation: Mutation) {
    let outcome = match &mutation {
        Mutation::Approve { name, root } => {
            approve_candidate(root, name).map(|_| format!("Approved {name}. It is now active."))
        }
        Mutation::Reject { name, root } => {
            reject_candidate(root, name).map(|_| format!("Rejected {name} (archived)."))
        }
        Mutation::Disable { name, root } => {
            archive_skill(root, name).map(|_| format!("Disabled {name} (archived, reversible)."))
        }
        Mutation::Restore { name, root } => {
            restore_skill(root, name).map(|_| format!("Restored {name}. It is now active."))
        }
    };
    match outcome {
        Ok(message) => model.set_status(message),
        Err(error) => model.set_status(error.to_string()),
    }
}
