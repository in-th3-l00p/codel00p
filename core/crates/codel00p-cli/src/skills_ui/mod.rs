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

use std::path::{Path, PathBuf};

use codel00p_skill::{
    Skill, SkillSource, approve_candidate, archive_skill, load_archived, load_candidates,
    load_skills, load_usage, reject_candidate, restore_skill,
};

use crate::config::CliResult;
use crate::skills::{skill_sources, user_skills_dir};
use model::{Flow, Mutation, SkillKind, SkillRow, SkillsModel};

/// Runs the review dialog. The terminal lifecycle and blocking loop come from
/// [`crate::dialog`]; the `on_key` closure performs the skill-store effects.
pub(crate) fn run(workspace_start: &Path) -> CliResult<String> {
    let mut model = SkillsModel::new();
    model.set_rows(load_rows(workspace_start));

    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Stay | Flow::OpenDetail => {}
            Flow::Quit => return Ok(false),
            Flow::Mutate(mutation) => {
                apply(model, mutation);
                model.screen = model::Screen::List;
                model.selected = None;
                model.set_rows(load_rows(workspace_start));
            }
        }
        Ok(true)
    })?;
    Ok("Closed skills review.\n".to_string())
}

/// Builds the combined active-skill + candidate row set for the model.
fn load_rows(workspace_start: &Path) -> Vec<SkillRow> {
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

    let mut rows: Vec<SkillRow> = Vec::new();
    for skill in &skills {
        let root = root_for(skill.source);
        let usage = load_usage(&root).get(&skill.name);
        rows.push(active_row(skill, root, usage.count));
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
