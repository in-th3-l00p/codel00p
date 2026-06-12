//! The `codel00p skills` command: list, show, and scaffold skills.
//!
//! Skills are loaded from layered directories — `~/.codel00p/skills` (user) and
//! the project's `.codel00p/skills` — with project skills overriding user ones
//! by name.

use std::{
    fs,
    path::{Path, PathBuf},
};

use codel00p_skill::{SKILL_FILE, SkillSource, load_skills};

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
        _ => Err(format!("unknown skills command: {command}")),
    }
}

fn user_skills_dir() -> PathBuf {
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

fn skill_sources(workspace_start: &Path) -> Vec<(SkillSource, PathBuf)> {
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

    let mut output = String::from("Skills:\n");
    for skill in &skills {
        let version = skill
            .version
            .as_deref()
            .map(|value| format!(" v{value}"))
            .unwrap_or_default();
        output.push_str(&format!(
            "  {:<20} [{}]{}  {}\n",
            skill.name,
            skill.source.label(),
            version,
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
