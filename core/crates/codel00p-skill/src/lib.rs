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
    #[error("no active skill named `{name}`")]
    UnknownSkill { name: String },
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

// --- Curation: retiring stale agent-created skills -------------------------
//
// The curator reversibly archives active skills the agent created that are not
// earning their place in context. Archived skills move to `<root>/.archive` and
// are no longer loaded (so never injected), but can be restored. Bundled and
// human-authored skills are never touched (see [`is_curatable`]).

/// The `.archive` directory of retired active skills under a skills root.
pub fn archive_root(skills_dir: &Path) -> PathBuf {
    skills_dir.join(ARCHIVE_DIR)
}

/// Load the skills archived (disabled) under a skills root.
///
/// Mirrors [`load_candidates`]: archived skills live under `<root>/.archive` and
/// are never loaded as active skills, but listing them lets a reviewer see and
/// [`restore_skill`] them without leaving the dialog.
pub fn load_archived(skills_dir: &Path) -> Vec<Skill> {
    scan_dir(&archive_root(skills_dir), SkillSource::User)
}

/// Reversibly archive an active skill, moving it out of the loaded set.
pub fn archive_skill(skills_dir: &Path, name: &str) -> Result<PathBuf, SkillError> {
    validate_name(name)?;
    let from = skills_dir.join(name);
    if !from.join(SKILL_FILE).is_file() {
        return Err(SkillError::UnknownSkill {
            name: name.to_string(),
        });
    }
    let to = archive_root(skills_dir).join(name);
    create_dir_all(&archive_root(skills_dir))?;
    if to.exists() {
        fs::remove_dir_all(&to).map_err(|source| SkillError::Io {
            path: to.clone(),
            source,
        })?;
    }
    rename(&from, &to)?;
    Ok(to.join(SKILL_FILE))
}

/// Restore a previously archived skill back into the active set.
pub fn restore_skill(skills_dir: &Path, name: &str) -> Result<PathBuf, SkillError> {
    validate_name(name)?;
    let from = archive_root(skills_dir).join(name);
    if !from.join(SKILL_FILE).is_file() {
        return Err(SkillError::UnknownSkill {
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

/// Whether a skill should be curated (reversibly archived): the agent created
/// it, it has never been used, and it is older than `min_age_secs`.
///
/// Human-authored or bundled skills are never curatable, and a grace period
/// (`min_age_secs`) keeps a freshly-approved skill from being archived before it
/// has had a chance to be used.
pub fn is_curatable(skill: &Skill, usage: SkillUsage, age_secs: u64, min_age_secs: u64) -> bool {
    skill.created_by.as_deref() == Some("agent") && usage.count == 0 && age_secs >= min_age_secs
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
mod tests;
