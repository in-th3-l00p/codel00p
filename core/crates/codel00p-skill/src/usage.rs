//! Skill usage tracking.
//!
//! Each skills root keeps a `.usage.json` recording how often each skill was
//! applied and when it was last used. This is the signal the self-improvement
//! curator uses to retire stale agent-created skills, and it powers usage
//! columns in `skills list`. It is intentionally simple (read-modify-write,
//! last-writer-wins) — fine for a local, single-user store.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{SKILL_FILE, Skill, SkillError};

/// The usage file under a skills root.
pub const USAGE_FILE: &str = ".usage.json";

/// How often a skill has been applied, and when it was last used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillUsage {
    #[serde(default)]
    pub count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_epoch: Option<u64>,
}

/// The usage log for a skills root: per-skill [`SkillUsage`], keyed by name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsageLog {
    skills: BTreeMap<String, SkillUsage>,
}

impl UsageLog {
    /// Usage for a skill (defaulting to zero if it has never been recorded).
    pub fn get(&self, name: &str) -> SkillUsage {
        self.skills.get(name).copied().unwrap_or_default()
    }

    fn record(&mut self, name: &str, now_epoch: u64) {
        let entry = self.skills.entry(name.to_string()).or_default();
        entry.count += 1;
        entry.last_used_epoch = Some(now_epoch);
    }
}

/// The `.usage.json` path under a skills root.
pub fn usage_path(skills_dir: &Path) -> PathBuf {
    skills_dir.join(USAGE_FILE)
}

/// Load the usage log for a skills root (empty if absent or unreadable).
pub fn load_usage(skills_dir: &Path) -> UsageLog {
    let path = usage_path(skills_dir);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Record one use of `name` under `skills_dir` at `now_epoch`.
pub fn record_usage(skills_dir: &Path, name: &str, now_epoch: u64) -> Result<(), SkillError> {
    let mut log = load_usage(skills_dir);
    log.record(name, now_epoch);

    let path = usage_path(skills_dir);
    let contents = serde_json::to_string_pretty(&log).expect("serialize usage log");
    std::fs::write(&path, contents).map_err(|source| SkillError::Io { path, source })
}

/// Record one use of a loaded skill, deriving its skills root from its path
/// (`<root>/<name>/SKILL.md`), so a skill's usage is logged in its own root.
pub fn record_skill_usage(skill: &Skill, now_epoch: u64) -> Result<(), SkillError> {
    if let Some(root) = skill_root(&skill.path) {
        record_usage(&root, &skill.name, now_epoch)?;
    }
    Ok(())
}

fn skill_root(skill_file: &Path) -> Option<PathBuf> {
    // <root>/<name>/SKILL.md -> <root>
    if skill_file.file_name()?.to_str()? == SKILL_FILE {
        skill_file.parent()?.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn records_and_loads_usage() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        assert_eq!(load_usage(root).get("deploy"), SkillUsage::default());

        record_usage(root, "deploy", 1000).unwrap();
        record_usage(root, "deploy", 2000).unwrap();
        record_usage(root, "lint", 1500).unwrap();

        let log = load_usage(root);
        assert_eq!(log.get("deploy").count, 2);
        assert_eq!(log.get("deploy").last_used_epoch, Some(2000));
        assert_eq!(log.get("lint").count, 1);
        assert_eq!(log.get("missing").count, 0);
    }

    #[test]
    fn record_skill_usage_logs_in_the_skills_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let skill_dir = root.join("deploy");
        fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join(SKILL_FILE);
        fs::write(&path, "---\nname: deploy\ndescription: d\n---\nbody\n").unwrap();

        let skill = crate::load_skill(&path, crate::SkillSource::User).unwrap();
        record_skill_usage(&skill, 4242).unwrap();

        // Usage is written to the root's .usage.json, not the skill dir.
        assert_eq!(load_usage(root).get("deploy").count, 1);
        assert_eq!(load_usage(root).get("deploy").last_used_epoch, Some(4242));
    }
}
