//! The `codel00p skills` command: list, show, and scaffold skills.
//!
//! Skills are loaded from layered directories — `~/.codel00p/skills` (user) and
//! the project's `.codel00p/skills` — with project skills overriding user ones
//! by name.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use codel00p_cron::parse_schedule;
use codel00p_skill::{
    DEFAULT_SKILL_CONSOLIDATION_THRESHOLD, SKILL_FILE, Skill, SkillConsolidation, SkillSource,
    approve_candidate, archive_skill, is_curatable, load_candidates, load_skills, load_usage,
    plan_skill_consolidations, reject_candidate,
};

use crate::{config::CliResult, settings};

/// Default grace period before an unused agent skill becomes curatable (7 days).
const DEFAULT_CURATE_MIN_AGE_SECS: u64 = 7 * 86_400;

pub fn run(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => {
            // Bare `codel00p skills` on a terminal opens the review dialog; pipes
            // and CI keep the scriptable `list` behavior so output is never
            // corrupted.
            use std::io::IsTerminal;
            return if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
                crate::skills_ui::run(workspace_start)
            } else {
                skills_list(workspace_start)
            };
        }
    };
    match command {
        "list" => skills_list(workspace_start),
        "show" => skills_show(workspace_start, rest),
        "create" => skills_create(workspace_start, rest),
        "candidates" => skills_candidates(workspace_start),
        "approve" => skills_review(workspace_start, rest, Review::Approve),
        "reject" => skills_review(workspace_start, rest, Review::Reject),
        "curate" => skills_curate(workspace_start, rest),
        _ => Err(format!("unknown skills command: {command}")),
    }
}

pub(crate) fn user_skills_dir() -> PathBuf {
    settings::home_dir().join("skills")
}

fn project_skills_dir(workspace_start: &Path) -> PathBuf {
    let config = settings::discover_project_config(workspace_start)
        .unwrap_or_else(|| settings::project_config_path(workspace_start));
    config
        .parent()
        .map(|dir| dir.join("skills"))
        .unwrap_or_else(|| workspace_start.join(".codel00p").join("skills"))
}

pub(crate) fn skill_sources(workspace_start: &Path) -> Vec<(SkillSource, PathBuf)> {
    vec![
        (SkillSource::User, user_skills_dir()),
        (SkillSource::Project, project_skills_dir(workspace_start)),
    ]
}

fn skills_list(workspace_start: &Path) -> CliResult<String> {
    let skills = load_skills(&skill_sources(workspace_start));
    if skills.is_empty() {
        return Ok("No skills found. Create one: codel00p skills create <name>\n".to_string());
    }

    let user_usage = load_usage(&user_skills_dir());
    let project_usage = load_usage(&project_skills_dir(workspace_start));

    let mut output = String::from("Skills:\n");
    for skill in &skills {
        let usage = match skill.source {
            SkillSource::Project => project_usage.get(&skill.name),
            _ => user_usage.get(&skill.name),
        };
        let used = if usage.count == 0 {
            "unused".to_string()
        } else {
            format!("used {}x", usage.count)
        };
        output.push_str(&format!(
            "  {:<20} [{}] {:<10} {}\n",
            skill.name,
            skill.source.label(),
            used,
            skill.description
        ));
    }
    output.push_str("\nShow one:  codel00p skills show <name>\n");
    Ok(output)
}

fn skills_show(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let name = args.first().ok_or("usage: skills show <name>")?;
    let skills = load_skills(&skill_sources(workspace_start));
    let skill = skills
        .iter()
        .find(|skill| &skill.name == name)
        .ok_or_else(|| format!("unknown skill: {name}"))?;

    let mut output = format!("{} ({})\n", skill.name, skill.source.label());
    if let Some(version) = &skill.version {
        output.push_str(&format!("  version:   {version}\n"));
    }
    if let Some(author) = &skill.author {
        output.push_str(&format!("  author:    {author}\n"));
    }
    if !skill.description.is_empty() {
        output.push_str(&format!("  summary:   {}\n", skill.description));
    }
    if !skill.triggers.is_empty() {
        output.push_str(&format!("  triggers:  {}\n", skill.triggers.join(", ")));
    }
    output.push_str(&format!("  path:      {}\n\n", skill.path.display()));
    output.push_str(&skill.body);
    output.push('\n');
    Ok(output)
}

struct SkillCreateOptions {
    name: String,
    project: bool,
}

fn parse_skill_create(args: &[String]) -> CliResult<SkillCreateOptions> {
    let mut name = None;
    let mut project = false;
    for arg in args {
        match arg.as_str() {
            "--project" => project = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown skills create option: {flag}"));
            }
            value => {
                if name.is_some() {
                    return Err(format!("unexpected argument: {value}"));
                }
                name = Some(value.to_string());
            }
        }
    }
    Ok(SkillCreateOptions {
        name: name.ok_or("usage: skills create <name> [--project]")?,
        project,
    })
}

fn skills_create(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let options = parse_skill_create(args)?;
    let base = if options.project {
        project_skills_dir(workspace_start)
    } else {
        user_skills_dir()
    };
    let dir = base.join(&options.name);
    let file = dir.join(SKILL_FILE);
    if file.exists() {
        return Err(format!("skill already exists: {}", file.display()));
    }

    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    let template = format!(
        "---\nname: {name}\nversion: 0.1.0\ndescription: \ntriggers:\n  - \n---\n# {name}\n\nDescribe when this skill applies and the steps to follow.\n",
        name = options.name
    );
    fs::write(&file, template)
        .map_err(|error| format!("failed to write {}: {error}", file.display()))?;

    Ok(format!(
        "Created skill {} at {}\n",
        options.name,
        file.display()
    ))
}

// --- review queue: agent-proposed skills -----------------------------------

fn skills_candidates(workspace_start: &Path) -> CliResult<String> {
    let mut candidates = load_candidates(&user_skills_dir());
    candidates.extend(load_candidates(&project_skills_dir(workspace_start)));
    if candidates.is_empty() {
        return Ok("No skill candidates awaiting review.\n".to_string());
    }

    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    let mut output = String::from("Skill candidates (awaiting review):\n");
    for skill in &candidates {
        let by = skill
            .created_by
            .as_deref()
            .map(|value| format!(" [{value}]"))
            .unwrap_or_default();
        output.push_str(&format!(
            "  {:<20}{}  {}\n",
            skill.name, by, skill.description
        ));
    }
    output.push_str(
        "\nApprove:  codel00p skills approve <name>\n\
         Reject:   codel00p skills reject <name>\n\
         Inspect:  codel00p skills show <name>   (after approving)\n",
    );
    Ok(output)
}

enum Review {
    Approve,
    Reject,
}

struct ReviewOptions {
    name: String,
    project: bool,
}

fn parse_review(args: &[String], verb: &str) -> CliResult<ReviewOptions> {
    let mut name = None;
    let mut project = false;
    for arg in args {
        match arg.as_str() {
            "--project" => project = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown skills {verb} option: {flag}"));
            }
            value => {
                if name.is_some() {
                    return Err(format!("unexpected argument: {value}"));
                }
                name = Some(value.to_string());
            }
        }
    }
    Ok(ReviewOptions {
        name: name.ok_or_else(|| format!("usage: skills {verb} <name> [--project]"))?,
        project,
    })
}

fn skills_review(workspace_start: &Path, args: &[String], action: Review) -> CliResult<String> {
    let verb = match action {
        Review::Approve => "approve",
        Review::Reject => "reject",
    };
    let options = parse_review(args, verb)?;
    let root = if options.project {
        project_skills_dir(workspace_start)
    } else {
        user_skills_dir()
    };

    match action {
        Review::Approve => {
            approve_candidate(&root, &options.name).map_err(|error| error.to_string())?;
            Ok(format!(
                "Approved skill {}. It will be applied on relevant future turns.\n",
                options.name
            ))
        }
        Review::Reject => {
            reject_candidate(&root, &options.name).map_err(|error| error.to_string())?;
            Ok(format!("Rejected skill {} (archived).\n", options.name))
        }
    }
}

// --- curation: retire stale agent-created skills ---------------------------

struct CurateOptions {
    apply: bool,
    min_age_secs: u64,
    threshold: u8,
}

fn parse_curate(args: &[String]) -> CliResult<CurateOptions> {
    let mut apply = false;
    let mut min_age_secs = DEFAULT_CURATE_MIN_AGE_SECS;
    let mut threshold = DEFAULT_SKILL_CONSOLIDATION_THRESHOLD;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--threshold" => {
                let value = args
                    .get(index + 1)
                    .cloned()
                    .filter(|v| !v.starts_with("--"))
                    .ok_or("missing value for --threshold")?;
                threshold = value
                    .parse::<u8>()
                    .ok()
                    .filter(|score| *score <= 100)
                    .ok_or("invalid --threshold")?;
                index += 2;
            }
            "--min-age" => {
                let value = args
                    .get(index + 1)
                    .cloned()
                    .filter(|v| !v.starts_with("--"))
                    .ok_or("missing value for --min-age")?;
                // A bare integer is seconds (so 0 disables the grace period);
                // otherwise it is a duration like 7d.
                min_age_secs = match value.parse::<u64>() {
                    Ok(secs) => secs,
                    Err(_) => parse_schedule(&value)
                        .map(|s| s.interval_secs())
                        .map_err(|error| error.to_string())?,
                };
                index += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown skills curate option: {flag}"));
            }
            value => return Err(format!("unexpected argument: {value}")),
        }
    }
    Ok(CurateOptions {
        apply,
        min_age_secs,
        threshold,
    })
}

fn skills_curate(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let options = parse_curate(args)?;
    let skills = load_skills(&skill_sources(workspace_start));
    let user_usage = load_usage(&user_skills_dir());
    let project_usage = load_usage(&project_skills_dir(workspace_start));
    let now = now_epoch();
    let usage_for = |skill: &Skill| match skill.source {
        SkillSource::Project => project_usage.get(&skill.name),
        _ => user_usage.get(&skill.name),
    };

    // Two independent passes: retire stale unused agent skills (always on, prior
    // behavior), and — only when the opt-in curator is enabled — consolidate
    // near-duplicate agent skills (keeping the most-used survivor). Both archive
    // reversibly; bundled/human skills are never touched.
    let stale: Vec<&Skill> = skills
        .iter()
        .filter(|skill| {
            is_curatable(
                skill,
                usage_for(skill),
                skill_age_secs(&skill.path, now),
                options.min_age_secs,
            )
        })
        .collect();
    let consolidations = if curator_enabled(workspace_start) {
        plan_skill_consolidations(&skills, usage_for, options.threshold)
    } else {
        Vec::new()
    };

    if stale.is_empty() && consolidations.is_empty() {
        return Ok(
            "No skills to curate: no stale agent skills and no near-duplicates.\n".to_string(),
        );
    }

    if !options.apply {
        return Ok(render_skill_curate_dry_run(
            &stale,
            &consolidations,
            options.threshold,
        ));
    }

    // Archive the union of stale skills and consolidation duplicates, each once.
    let mut archived: Vec<String> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut archive_skill_once = |skill: &Skill| -> CliResult<()> {
        if !seen.insert(skill.name.clone()) {
            return Ok(());
        }
        if let Some(root) = skill_root(&skill.path) {
            archive_skill(&root, &skill.name).map_err(|error| error.to_string())?;
            archived.push(skill.name.clone());
        }
        Ok(())
    };
    for skill in &stale {
        archive_skill_once(skill)?;
    }
    for consolidation in &consolidations {
        for duplicate in consolidation.duplicates() {
            archive_skill_once(duplicate.skill())?;
        }
    }

    Ok(format!(
        "Archived {} skill(s): {}\nRestore with the skill files under <root>/.archive.\n",
        archived.len(),
        archived.join(", ")
    ))
}

fn render_skill_curate_dry_run(
    stale: &[&Skill],
    consolidations: &[SkillConsolidation],
    threshold: u8,
) -> String {
    let mut output = String::new();
    if !stale.is_empty() {
        output.push_str("Stale agent-created skills (unused past the grace period):\n");
        for skill in stale {
            output.push_str(&format!("  {}\n", skill.name));
        }
        output.push('\n');
    }
    if !consolidations.is_empty() {
        output.push_str(&format!(
            "Near-duplicate agent skills (\u{2265}{threshold}% similar):\n"
        ));
        for consolidation in consolidations {
            output.push_str(&format!("  keep {}\n", consolidation.survivor().name));
            for duplicate in consolidation.duplicates() {
                output.push_str(&format!(
                    "    archive {} ({}% similar)\n",
                    duplicate.skill().name,
                    duplicate.similarity()
                ));
            }
        }
        output.push('\n');
    }
    output.push_str("Archive them (reversible):  codel00p skills curate --apply\n");
    output
}

/// Whether the opt-in curator is enabled in the layered configuration. Any
/// resolution failure is treated as disabled so consolidation never runs
/// unexpectedly.
pub(crate) fn curator_enabled(workspace_start: &Path) -> bool {
    settings::load_layered(workspace_start)
        .map(|resolved| resolved.merged.agent.behavior.curator_enabled())
        .unwrap_or(false)
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Age of a skill file in seconds (0 if its mtime can't be read, so an
/// unreadable mtime is treated as new and never curated).
fn skill_age_secs(path: &Path, now: u64) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|mtime| mtime.duration_since(UNIX_EPOCH).ok())
        .map(|d| now.saturating_sub(d.as_secs()))
        .unwrap_or(0)
}

/// `<root>/<name>/SKILL.md` -> `<root>`.
fn skill_root(skill_file: &Path) -> Option<PathBuf> {
    skill_file.parent()?.parent().map(Path::to_path_buf)
}
