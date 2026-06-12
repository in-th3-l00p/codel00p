//! The `codel00p skills` command: list, show, and scaffold skills.
//!
//! Skills are loaded from layered directories — `~/.codel00p/skills` (user) and
//! the project's `.codel00p/skills` — with project skills overriding user ones
//! by name.

use std::{
    fs,
    path::{Path, PathBuf},
};

use codel00p_skill::{
    SKILL_FILE, SkillSource, approve_candidate, load_candidates, load_skills, load_usage,
    reject_candidate,
};

use crate::{config::CliResult, settings};

pub fn run(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("list", &[][..]),
    };
    match command {
        "list" => skills_list(workspace_start),
        "show" => skills_show(workspace_start, rest),
        "create" => skills_create(workspace_start, rest),
        "candidates" => skills_candidates(workspace_start),
        "approve" => skills_review(workspace_start, rest, Review::Approve),
        "reject" => skills_review(workspace_start, rest, Review::Reject),
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
