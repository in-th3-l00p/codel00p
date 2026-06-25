//! Deterministic, offline consolidation planning for near-duplicate skills.
//!
//! Mirrors the memory curator: this plans (never applies) consolidations over a
//! set of loaded skills and returns them for review. A consolidation is a cluster
//! of near-duplicate **agent-authored** skills (shingle similarity at or above a
//! threshold) reduced to a single **survivor** plus the **duplicates** that would
//! be archived if the plan is applied.
//!
//! Only agent-authored skills (`created_by == "agent"`) are ever considered — the
//! same guard [`crate::is_curatable`] uses — so human and bundled skills are never
//! proposed for archiving. Detection is the offline shingle/Jaccard similarity (no
//! LLM), so consolidation keeps the best existing skill and archives the rest
//! rather than synthesizing a merged one. Survivor selection is deterministic:
//! the most-used skill wins, ties broken by the lexicographically smallest name.

use codel00p_textsim::shingle_similarity;

use crate::{Skill, SkillUsage};

/// Default similarity threshold (0..=100) at or above which two agent-authored
/// skills are treated as near-duplicates worth consolidating.
pub const DEFAULT_SKILL_CONSOLIDATION_THRESHOLD: u8 = 60;

/// A planned consolidation of a near-duplicate skill cluster: one surviving skill
/// and the duplicates that would be archived as redundant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillConsolidation {
    survivor: Skill,
    duplicates: Vec<DuplicateSkill>,
}

/// A skill the curator would archive in favor of a cluster's survivor, carrying
/// its similarity (0..=100) to that survivor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateSkill {
    skill: Skill,
    similarity: u8,
}

impl SkillConsolidation {
    /// The skill kept active for this cluster (most-used; ties → smallest name).
    pub fn survivor(&self) -> &Skill {
        &self.survivor
    }

    /// The near-duplicate skills that would be archived, ordered by name.
    pub fn duplicates(&self) -> &[DuplicateSkill] {
        &self.duplicates
    }
}

impl DuplicateSkill {
    pub fn skill(&self) -> &Skill {
        &self.skill
    }

    /// Similarity (0..=100) of this duplicate to the cluster's survivor.
    pub fn similarity(&self) -> u8 {
        self.similarity
    }
}

/// The text compared for similarity: the description plus the instructions body,
/// so two skills that describe and do the same thing score as near-duplicates.
fn skill_text(skill: &Skill) -> String {
    format!("{}\n{}", skill.description, skill.body)
}

/// Plans consolidations over the **agent-authored** skills in `skills`, treating
/// two as near-duplicates when their shingle similarity is at least `threshold`.
/// `usage_for` supplies each skill's usage so the most-used survives. Only
/// clusters of two or more are returned; singletons are left alone.
///
/// Pure and deterministic: callers pass the skills to consider and apply the plan
/// separately by archiving each duplicate.
pub fn plan_skill_consolidations(
    skills: &[Skill],
    usage_for: impl Fn(&Skill) -> SkillUsage,
    threshold: u8,
) -> Vec<SkillConsolidation> {
    // Only agent-authored skills are curatable; sort by name so clustering and
    // output are stable.
    let mut candidates: Vec<&Skill> = skills
        .iter()
        .filter(|skill| skill.created_by.as_deref() == Some("agent"))
        .collect();
    candidates.sort_by(|left, right| left.name.cmp(&right.name));

    let count = candidates.len();
    let mut parent: Vec<usize> = (0..count).collect();
    for left in 0..count {
        for right in (left + 1)..count {
            let similarity =
                shingle_similarity(&skill_text(candidates[left]), &skill_text(candidates[right]));
            if similarity >= threshold {
                union(&mut parent, left, right);
            }
        }
    }

    // Group candidate positions by cluster root (insertion order stays name-sorted).
    let mut clusters: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for position in 0..count {
        let root = find(&mut parent, position);
        clusters.entry(root).or_default().push(position);
    }

    let mut consolidations = Vec::new();
    for (_root, mut members) in clusters {
        if members.len() < 2 {
            continue;
        }
        // Survivor: most-used, ties broken by smallest name.
        members.sort_by(|&left, &right| {
            usage_for(candidates[right])
                .count
                .cmp(&usage_for(candidates[left]).count)
                .then_with(|| candidates[left].name.cmp(&candidates[right].name))
        });
        let survivor = candidates[members[0]].clone();

        let mut duplicates: Vec<DuplicateSkill> = members[1..]
            .iter()
            .map(|&member| DuplicateSkill {
                skill: candidates[member].clone(),
                similarity: shingle_similarity(&skill_text(&survivor), &skill_text(candidates[member])),
            })
            .collect();
        duplicates.sort_by(|left, right| left.skill.name.cmp(&right.skill.name));

        consolidations.push(SkillConsolidation {
            survivor,
            duplicates,
        });
    }

    consolidations.sort_by(|left, right| left.survivor.name.cmp(&right.survivor.name));
    consolidations
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find(parent, left);
    let right_root = find(parent, right);
    if left_root == right_root {
        return;
    }
    if left_root < right_root {
        parent[right_root] = left_root;
    } else {
        parent[left_root] = right_root;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SkillSource;
    use std::path::PathBuf;

    fn skill(name: &str, created_by: &str, description: &str, body: &str) -> Skill {
        Skill {
            name: name.to_string(),
            version: None,
            description: description.to_string(),
            author: None,
            triggers: Vec::new(),
            created_by: Some(created_by.to_string()),
            source: SkillSource::User,
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            body: body.to_string(),
        }
    }

    fn no_usage(_: &Skill) -> SkillUsage {
        SkillUsage::default()
    }

    #[test]
    fn clusters_near_duplicate_agent_skills_and_keeps_most_used() {
        let skills = vec![
            skill(
                "deploy-staging",
                "agent",
                "Deploy the app to staging",
                "run the deploy script and verify the staging health check passes",
            ),
            skill(
                "ship-staging",
                "agent",
                "Deploy the app to staging environment",
                "run the deploy script and verify the staging health check is passing",
            ),
        ];
        // ship-staging has been used; deploy-staging never → ship survives.
        let usage = |skill: &Skill| {
            if skill.name == "ship-staging" {
                SkillUsage {
                    count: 3,
                    last_used_epoch: Some(100),
                }
            } else {
                SkillUsage::default()
            }
        };
        let plan = plan_skill_consolidations(&skills, usage, 40);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].survivor().name, "ship-staging");
        assert_eq!(plan[0].duplicates().len(), 1);
        assert_eq!(plan[0].duplicates()[0].skill().name, "deploy-staging");
        assert!(plan[0].duplicates()[0].similarity() >= 40);
    }

    #[test]
    fn human_and_bundled_skills_are_never_considered() {
        let skills = vec![
            skill(
                "user-deploy",
                "user",
                "Deploy the app to staging",
                "run the deploy script and verify the staging health check passes",
            ),
            skill(
                "agent-deploy",
                "agent",
                "Deploy the app to staging environment",
                "run the deploy script and verify the staging health check is passing",
            ),
        ];
        // Only one agent skill → no cluster, the human duplicate is untouched.
        assert!(plan_skill_consolidations(&skills, no_usage, 40).is_empty());
    }

    #[test]
    fn unrelated_skills_produce_no_plan() {
        let skills = vec![
            skill("deploy", "agent", "Deploy the app", "run the deploy script for production"),
            skill("lint", "agent", "Lint the code", "run the formatter and the linter over the tree"),
        ];
        assert!(plan_skill_consolidations(&skills, no_usage, 60).is_empty());
    }

    #[test]
    fn tie_on_usage_breaks_to_smallest_name() {
        let skills = vec![
            skill(
                "b-deploy",
                "agent",
                "Deploy the app to staging",
                "run the deploy script and verify the staging health check passes",
            ),
            skill(
                "a-deploy",
                "agent",
                "Deploy the app to staging environment",
                "run the deploy script and verify the staging health check is passing",
            ),
        ];
        let plan = plan_skill_consolidations(&skills, no_usage, 40);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].survivor().name, "a-deploy");
        assert_eq!(plan[0].duplicates()[0].skill().name, "b-deploy");
    }
}
