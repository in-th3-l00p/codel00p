//! Skills: procedural memory codel00p can load and apply.
//!
//! A skill is a directory containing a `SKILL.md` file: YAML-style front matter
//! (delimited by `---`) describing the skill, followed by Markdown instructions.
//! This crate defines the [`Skill`] model and a layered [`load_skills`] loader
//! (project overrides user overrides bundled), the first slice of the
//! [Skills initiative](../../../docs/initiatives/skills-system.md).
//!
//! The front-matter parser handles the simple shape skills use in practice —
//! scalar fields, block lists (`- item`), and inline lists (`[a, b]`) — rather
//! than arbitrary YAML, so the crate stays dependency-light. Relevance selection
//! and turn-context injection build on this model in later slices.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use thiserror::Error;

pub mod usage;
pub use usage::{
    SkillUsage, USAGE_FILE, UsageLog, load_usage, record_skill_usage, record_usage, usage_path,
};

/// The canonical file name inside a skill directory.
pub const SKILL_FILE: &str = "SKILL.md";

/// Subdirectory of a skills root holding proposed (unreviewed) skills.
pub const CANDIDATES_DIR: &str = ".candidates";
/// Subdirectory of the candidates dir holding rejected proposals.
pub const ARCHIVE_DIR: &str = ".archive";

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("failed to read skill at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("skill {path} is missing front matter delimited by `---`")]
    MissingFrontMatter { path: PathBuf },
    #[error("skill {path} has no `name` and its directory name could not be used")]
    MissingName { path: PathBuf },
    #[error("a skill named `{name}` is already active")]
    AlreadyActive { name: String },
    #[error("a candidate named `{name}` is already awaiting review")]
    CandidateExists { name: String },
    #[error("no candidate named `{name}` awaiting review")]
    UnknownCandidate { name: String },
    #[error("invalid skill name `{name}`: use letters, digits, `-`, `_`")]
    InvalidName { name: String },
}

/// Where a skill was loaded from, lowest to highest precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Bundled,
    User,
    Project,
}

impl SkillSource {
    pub fn label(self) -> &'static str {
        match self {
            SkillSource::Bundled => "bundled",
            SkillSource::User => "user",
            SkillSource::Project => "project",
        }
    }
}

/// A loaded skill: its metadata plus the Markdown instructions body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skill {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub triggers: Vec<String>,
    /// Provenance: who authored the skill (`agent` or `user`), if recorded.
    /// The curator keys off `agent` to retire stale machine-proposed skills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub source: SkillSource,
    pub path: PathBuf,
    #[serde(skip)]
    pub body: String,
}

/// Load every skill found under the given sources, with later sources (higher
/// precedence) overriding same-named skills from earlier ones.
///
/// Each source is a directory whose immediate subdirectories may contain a
/// `SKILL.md`. Unreadable or malformed skill files are skipped.
pub fn load_skills(sources: &[(SkillSource, PathBuf)]) -> Vec<Skill> {
    let mut ordered = sources.to_vec();
    ordered.sort_by_key(|(source, _)| *source);

    let mut by_name: BTreeMap<String, Skill> = BTreeMap::new();
    for (source, dir) in ordered {
        for skill in scan_dir(&dir, source) {
            by_name.insert(skill.name.clone(), skill);
        }
    }
    by_name.into_values().collect()
}

/// Select up to `limit` skills relevant to `query`, most relevant first.
///
/// Relevance is a deterministic score: each `trigger` (or the skill name) that
/// appears as a case-insensitive substring of the query counts once. Skills with
/// no match are excluded, so an empty or unrelated query selects nothing.
pub fn select_skills(skills: &[Skill], query: &str, limit: usize) -> Vec<Skill> {
    let haystack = query.to_lowercase();
    let mut scored: Vec<(usize, &Skill)> = skills
        .iter()
        .filter_map(|skill| {
            let score = relevance_score(skill, &haystack);
            (score > 0).then_some((score, skill))
        })
        .collect();
    // Higher score first; ties broken by name for determinism.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill.clone())
        .collect()
}

fn relevance_score(skill: &Skill, lowercase_query: &str) -> usize {
    let mut score = 0;
    for trigger in &skill.triggers {
        let trigger = trigger.trim().to_lowercase();
        if !trigger.is_empty() && lowercase_query.contains(&trigger) {
            score += 1;
        }
    }
    let name = skill.name.to_lowercase();
    if !name.is_empty() && lowercase_query.contains(&name) {
        score += 1;
    }
    score
}

/// Load a single `SKILL.md` at `path` with the given source.
pub fn load_skill(path: &Path, source: SkillSource) -> Result<Skill, SkillError> {
    let content = fs::read_to_string(path).map_err(|error| SkillError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    parse_skill(path, source, &content)
}

// --- Candidate lifecycle ---------------------------------------------------
//
// Self-improvement is review-gated: the agent *proposes* skills, humans (or
// policy) *approve* them. Proposals live under `<skills_root>/.candidates/` and
// are never loaded as active skills, so an unreviewed proposal can never reach a
// future turn's context. Approving moves the proposal into the active set;
// rejecting archives it (reversible).

/// A proposed skill awaiting review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillProposal {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub instructions: String,
    /// `agent` for machine-proposed skills, `user` for human-authored ones.
    pub created_by: String,
}

/// The `.candidates` directory under a skills root.
pub fn candidates_root(skills_dir: &Path) -> PathBuf {
    skills_dir.join(CANDIDATES_DIR)
}

/// Write a proposal as a review candidate, returning its `SKILL.md` path.
///
/// Fails if an active skill or an existing candidate already uses the name, so a
/// proposal never silently overwrites reviewed work.
pub fn propose_skill(skills_dir: &Path, proposal: &SkillProposal) -> Result<PathBuf, SkillError> {
    validate_name(&proposal.name)?;

    if skills_dir.join(&proposal.name).join(SKILL_FILE).is_file() {
        return Err(SkillError::AlreadyActive {
            name: proposal.name.clone(),
        });
    }
    let dir = candidates_root(skills_dir).join(&proposal.name);
    let file = dir.join(SKILL_FILE);
    if file.is_file() {
        return Err(SkillError::CandidateExists {
            name: proposal.name.clone(),
        });
    }

    create_dir_all(&dir)?;
    write_file(&file, &render_skill_md(proposal))?;
    Ok(file)
}

/// Load the skills awaiting review under a skills root.
pub fn load_candidates(skills_dir: &Path) -> Vec<Skill> {
    scan_dir(&candidates_root(skills_dir), SkillSource::User)
}

/// Approve a candidate, moving it into the active skill set so it is loaded and
/// injected on future turns.
pub fn approve_candidate(skills_dir: &Path, name: &str) -> Result<PathBuf, SkillError> {
    validate_name(name)?;
    let from = candidates_root(skills_dir).join(name);
    if !from.join(SKILL_FILE).is_file() {
        return Err(SkillError::UnknownCandidate {
            name: name.to_string(),
        });
    }
    let to = skills_dir.join(name);
    if to.join(SKILL_FILE).is_file() {
        return Err(SkillError::AlreadyActive {
            name: name.to_string(),
        });
    }
    rename(&from, &to)?;
    Ok(to.join(SKILL_FILE))
}

/// Reject a candidate, archiving it under `.candidates/.archive` (reversible).
pub fn reject_candidate(skills_dir: &Path, name: &str) -> Result<(), SkillError> {
    validate_name(name)?;
    let from = candidates_root(skills_dir).join(name);
    if !from.join(SKILL_FILE).is_file() {
        return Err(SkillError::UnknownCandidate {
            name: name.to_string(),
        });
    }
    let archive = candidates_root(skills_dir).join(ARCHIVE_DIR).join(name);
    if let Some(parent) = archive.parent() {
        create_dir_all(parent)?;
    }
    if archive.exists() {
        fs::remove_dir_all(&archive).map_err(|source| SkillError::Io {
            path: archive.clone(),
            source,
        })?;
    }
    rename(&from, &archive)?;
    Ok(())
}

/// Skill names become directory names, so keep them filesystem- and
/// review-safe: no separators, traversal, or surprises.
fn validate_name(name: &str) -> Result<(), SkillError> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(SkillError::InvalidName {
            name: name.to_string(),
        })
    }
}

fn render_skill_md(proposal: &SkillProposal) -> String {
    let description = sanitize_inline(&proposal.description);
    let mut out = String::from("---\n");
    out.push_str(&format!("name: {}\n", proposal.name));
    out.push_str(&format!("description: \"{description}\"\n"));
    out.push_str(&format!("created_by: {}\n", proposal.created_by));
    if !proposal.triggers.is_empty() {
        out.push_str("triggers:\n");
        for trigger in &proposal.triggers {
            out.push_str(&format!("  - {}\n", sanitize_inline(trigger)));
        }
    }
    out.push_str("---\n");
    out.push_str(proposal.instructions.trim());
    out.push('\n');
    out
}

/// Flatten to a single line and drop double quotes, so a value is safe inside
/// the simple front-matter the loader parses back.
fn sanitize_inline(value: &str) -> String {
    value.replace(['\n', '\r'], " ").replace('"', "'")
}

fn create_dir_all(path: &Path) -> Result<(), SkillError> {
    fs::create_dir_all(path).map_err(|source| SkillError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, contents: &str) -> Result<(), SkillError> {
    fs::write(path, contents).map_err(|source| SkillError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn rename(from: &Path, to: &Path) -> Result<(), SkillError> {
    fs::rename(from, to).map_err(|source| SkillError::Io {
        path: from.to_path_buf(),
        source,
    })
}

fn scan_dir(dir: &Path, source: SkillSource) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let skill_file = entry.path().join(SKILL_FILE);
        if skill_file.is_file()
            && let Ok(skill) = load_skill(&skill_file, source)
        {
            skills.push(skill);
        }
    }
    skills
}

fn parse_skill(path: &Path, source: SkillSource, content: &str) -> Result<Skill, SkillError> {
    let (front_matter, body) =
        split_front_matter(content).ok_or_else(|| SkillError::MissingFrontMatter {
            path: path.to_path_buf(),
        })?;
    let map = parse_front_matter(&front_matter);

    let name = scalar(&map, "name")
        .map(str::to_string)
        .or_else(|| directory_name(path))
        .ok_or_else(|| SkillError::MissingName {
            path: path.to_path_buf(),
        })?;

    Ok(Skill {
        name,
        version: scalar(&map, "version").map(str::to_string),
        description: scalar(&map, "description").unwrap_or("").to_string(),
        author: scalar(&map, "author").map(str::to_string),
        triggers: list(&map, "triggers"),
        created_by: scalar(&map, "created_by").map(str::to_string),
        source,
        path: path.to_path_buf(),
        body: body.trim().to_string(),
    })
}

/// The directory name containing a `<dir>/SKILL.md` path, used as a fallback id.
fn directory_name(skill_file: &Path) -> Option<String> {
    skill_file
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_string())
}

/// Split `---`-delimited front matter from the Markdown body.
fn split_front_matter(content: &str) -> Option<(String, String)> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = normalized.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }

    let mut front_matter = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        front_matter.push(line);
    }
    if !closed {
        return None;
    }

    let body: Vec<&str> = lines.collect();
    Some((front_matter.join("\n"), body.join("\n")))
}

enum FrontMatterValue {
    Scalar(String),
    List(Vec<String>),
}

fn parse_front_matter(text: &str) -> BTreeMap<String, FrontMatterValue> {
    let mut map = BTreeMap::new();
    let mut pending_list: Option<(String, Vec<String>)> = None;

    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(item) = trimmed.strip_prefix("- ") {
            if let Some((_, items)) = pending_list.as_mut() {
                items.push(unquote(item.trim()).to_string());
            }
            continue;
        }

        if let Some((key, items)) = pending_list.take() {
            map.insert(key, FrontMatterValue::List(items));
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();

        if value.is_empty() {
            pending_list = Some((key, Vec::new()));
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            let items = inner
                .split(',')
                .map(|item| unquote(item.trim()).to_string())
                .filter(|item| !item.is_empty())
                .collect();
            map.insert(key, FrontMatterValue::List(items));
        } else {
            map.insert(key, FrontMatterValue::Scalar(unquote(value).to_string()));
        }
    }

    if let Some((key, items)) = pending_list.take() {
        map.insert(key, FrontMatterValue::List(items));
    }
    map
}

fn scalar<'a>(map: &'a BTreeMap<String, FrontMatterValue>, key: &str) -> Option<&'a str> {
    match map.get(key) {
        Some(FrontMatterValue::Scalar(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn list(map: &BTreeMap<String, FrontMatterValue>, key: &str) -> Vec<String> {
    match map.get(key) {
        Some(FrontMatterValue::List(items)) => items.clone(),
        Some(FrontMatterValue::Scalar(value)) if !value.is_empty() => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_skill(root: &Path, dir: &str, contents: &str) {
        let skill_dir = root.join(dir);
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join(SKILL_FILE), contents).expect("write SKILL.md");
    }

    #[test]
    fn parses_front_matter_and_body() {
        let dir = tempdir().expect("tempdir");
        write_skill(
            dir.path(),
            "deploy",
            "---\nname: deploy\nversion: 1.2.0\ndescription: \"Ship the app\"\nauthor: ada\ntriggers:\n  - deploy\n  - release\n---\n# Deploy\n\nRun the deploy steps.\n",
        );

        let skill = load_skill(
            &dir.path().join("deploy").join(SKILL_FILE),
            SkillSource::User,
        )
        .expect("load");

        assert_eq!(skill.name, "deploy");
        assert_eq!(skill.version.as_deref(), Some("1.2.0"));
        assert_eq!(skill.description, "Ship the app");
        assert_eq!(skill.author.as_deref(), Some("ada"));
        assert_eq!(skill.triggers, vec!["deploy", "release"]);
        assert_eq!(skill.source, SkillSource::User);
        assert!(skill.body.starts_with("# Deploy"));
        assert!(skill.body.ends_with("Run the deploy steps."));
    }

    #[test]
    fn supports_inline_trigger_lists_and_name_fallback() {
        let dir = tempdir().expect("tempdir");
        // No `name` field — falls back to the directory name.
        write_skill(
            dir.path(),
            "lint-fix",
            "---\ndescription: Fix lints\ntriggers: [lint, format]\n---\nBody.\n",
        );

        let skill = load_skill(
            &dir.path().join("lint-fix").join(SKILL_FILE),
            SkillSource::Project,
        )
        .expect("load");
        assert_eq!(skill.name, "lint-fix");
        assert_eq!(skill.triggers, vec!["lint", "format"]);
    }

    #[test]
    fn missing_front_matter_is_an_error() {
        let dir = tempdir().expect("tempdir");
        write_skill(dir.path(), "bad", "no front matter here\n");
        let error =
            load_skill(&dir.path().join("bad").join(SKILL_FILE), SkillSource::User).unwrap_err();
        assert!(matches!(error, SkillError::MissingFrontMatter { .. }));
    }

    #[test]
    fn selects_skills_by_trigger_relevance() {
        let dir = tempdir().expect("tempdir");
        write_skill(
            dir.path(),
            "deploy",
            "---\nname: deploy\ndescription: d\ntriggers: [deploy, ship]\n---\nbody\n",
        );
        write_skill(
            dir.path(),
            "lint",
            "---\nname: lint\ndescription: d\ntriggers: [lint]\n---\nbody\n",
        );
        let skills = load_skills(&[(SkillSource::User, dir.path().to_path_buf())]);

        let selected = select_skills(&skills, "please deploy the app", 5);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "deploy");

        // Unrelated query selects nothing.
        assert!(select_skills(&skills, "write a poem", 5).is_empty());

        // Limit is respected.
        let both = select_skills(&skills, "deploy and lint", 1);
        assert_eq!(both.len(), 1);
    }

    fn sample_proposal(name: &str) -> SkillProposal {
        SkillProposal {
            name: name.to_string(),
            description: "Ship the app safely".to_string(),
            triggers: vec!["deploy".to_string(), "release".to_string()],
            instructions: "1. Run tests.\n2. Deploy.\n3. Smoke test.".to_string(),
            created_by: "agent".to_string(),
        }
    }

    #[test]
    fn proposed_skill_is_a_candidate_not_an_active_skill() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        propose_skill(root, &sample_proposal("deploy")).expect("propose");

        // Not visible as an active skill...
        assert!(load_skills(&[(SkillSource::User, root.to_path_buf())]).is_empty());
        // ...but visible as a candidate, with provenance preserved.
        let candidates = load_candidates(root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "deploy");
        assert_eq!(candidates[0].created_by.as_deref(), Some("agent"));
        assert_eq!(candidates[0].triggers, vec!["deploy", "release"]);
    }

    #[test]
    fn proposal_is_rejected_when_name_is_taken() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();

        propose_skill(root, &sample_proposal("deploy")).expect("first propose");
        // A second proposal of the same name is a duplicate candidate.
        assert!(matches!(
            propose_skill(root, &sample_proposal("deploy")),
            Err(SkillError::CandidateExists { .. })
        ));

        // And an active skill blocks proposing the same name.
        write_skill(
            root,
            "active",
            "---\nname: active\ndescription: d\n---\nbody\n",
        );
        assert!(matches!(
            propose_skill(root, &sample_proposal("active")),
            Err(SkillError::AlreadyActive { .. })
        ));
    }

    #[test]
    fn proposal_rejects_unsafe_names() {
        let dir = tempdir().expect("tempdir");
        assert!(matches!(
            propose_skill(dir.path(), &sample_proposal("../escape")),
            Err(SkillError::InvalidName { .. })
        ));
    }

    #[test]
    fn approving_a_candidate_activates_it() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        propose_skill(root, &sample_proposal("deploy")).expect("propose");

        approve_candidate(root, "deploy").expect("approve");

        // Now an active skill, no longer a candidate.
        let active = load_skills(&[(SkillSource::User, root.to_path_buf())]);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "deploy");
        assert!(load_candidates(root).is_empty());

        // Approving an unknown candidate errors.
        assert!(matches!(
            approve_candidate(root, "missing"),
            Err(SkillError::UnknownCandidate { .. })
        ));
    }

    #[test]
    fn rejecting_a_candidate_archives_it() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        propose_skill(root, &sample_proposal("deploy")).expect("propose");

        reject_candidate(root, "deploy").expect("reject");

        // Gone from the review queue and never activated...
        assert!(load_candidates(root).is_empty());
        assert!(load_skills(&[(SkillSource::User, root.to_path_buf())]).is_empty());
        // ...but preserved in the archive for recovery.
        assert!(
            candidates_root(root)
                .join(ARCHIVE_DIR)
                .join("deploy")
                .join(SKILL_FILE)
                .is_file()
        );
    }

    #[test]
    fn project_skills_override_user_skills_by_name() {
        let user = tempdir().expect("user dir");
        let project = tempdir().expect("project dir");
        write_skill(
            user.path(),
            "deploy",
            "---\nname: deploy\ndescription: user version\n---\nuser body\n",
        );
        write_skill(
            project.path(),
            "deploy",
            "---\nname: deploy\ndescription: project version\n---\nproject body\n",
        );
        write_skill(
            user.path(),
            "test",
            "---\nname: test\ndescription: only in user\n---\nbody\n",
        );

        let skills = load_skills(&[
            (SkillSource::User, user.path().to_path_buf()),
            (SkillSource::Project, project.path().to_path_buf()),
        ]);

        // Sorted by name: deploy (project wins) then test (user only).
        assert_eq!(skills.len(), 2);
        let deploy = skills.iter().find(|s| s.name == "deploy").unwrap();
        assert_eq!(deploy.description, "project version");
        assert_eq!(deploy.source, SkillSource::Project);
        let test = skills.iter().find(|s| s.name == "test").unwrap();
        assert_eq!(test.source, SkillSource::User);
    }
}
