use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    config::CliResult,
    providers,
    settings::{self, ProfileSettings},
};

pub fn run(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("show", &[][..]),
    };
    match command {
        "show" => config_show(workspace_start, rest),
        "path" => config_path(workspace_start, rest),
        "get" => config_get(workspace_start, rest),
        "set" => config_set(workspace_start, rest),
        "unset" => config_unset(workspace_start, rest),
        "init" => config_init(workspace_start, rest),
        "edit" => config_edit(workspace_start, rest),
        "reset" => config_reset(rest),
        "profiles" => config_profiles(workspace_start, rest),
        "setup" => providers::setup(workspace_start),
        "migrate" => config_migrate(rest),
        _ => Err(format!("unknown config command: {command}")),
    }
}

fn split_project_flag(args: &[String]) -> (bool, Vec<String>) {
    let mut project = false;
    let mut rest = Vec::new();
    for arg in args {
        if arg == "--project" {
            project = true;
        } else {
            rest.push(arg.clone());
        }
    }
    (project, rest)
}

fn target_path(workspace_start: &Path, project: bool) -> PathBuf {
    if project {
        settings::project_config_path(workspace_start)
    } else {
        settings::user_config_path()
    }
}

fn config_show(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let json = args.iter().any(|arg| arg == "--json");
    let raw = args.iter().any(|arg| arg == "--raw");
    let resolved = settings::load_layered(workspace_start)?;

    if json {
        return serde_json::to_string_pretty(&resolved.merged)
            .map(|text| format!("{text}\n"))
            .map_err(|error| error.to_string());
    }
    if raw {
        return Ok(fs::read_to_string(&resolved.user_path).unwrap_or_default());
    }

    let agent = resolved.agent();
    let mut output = String::from("codel00p configuration\n");
    output.push_str(&format!(
        "  config file:  {}\n",
        resolved.user_path.display()
    ));
    if let Some(path) = &resolved.project_path {
        output.push_str(&format!("  project file: {}\n", path.display()));
    }
    output.push_str(&format!("  organization: {}\n", resolved.organization_id()));
    output.push_str(&format!(
        "  project:      {} ({})\n",
        resolved.project_id(),
        resolved.project_name()
    ));
    output.push_str(&format!(
        "  memory db:    {}\n",
        resolved.memory_db().display()
    ));
    output.push_str("\nagent defaults\n");
    output.push_str(&format!(
        "  provider:     {}\n",
        agent.provider.as_deref().unwrap_or("(unset)")
    ));
    output.push_str(&format!(
        "  model:        {}\n",
        agent.model.as_deref().unwrap_or("(unset)")
    ));
    if let Some(base_url) = &agent.base_url {
        output.push_str(&format!("  base url:     {base_url}\n"));
    }
    output.push_str(&format!(
        "  permission:   {}\n",
        agent.permission_mode.as_deref().unwrap_or("allow")
    ));
    output.push_str(&format!(
        "  streaming:    {}\n",
        agent.stream.unwrap_or(false)
    ));
    if let Some(tool_sets) = &agent.tool_sets {
        output.push_str(&format!("  tool sets:    {}\n", tool_sets.join(", ")));
    }
    Ok(output)
}

fn config_path(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (project, _) = split_project_flag(args);
    let path = if project {
        settings::project_config_path(workspace_start)
    } else {
        settings::user_config_path()
    };
    Ok(format!("{}\n", path.display()))
}

fn config_get(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let key = args
        .first()
        .ok_or_else(|| "usage: config get <key>".to_string())?;
    let resolved = settings::load_layered(workspace_start)?;
    match settings::effective_value(&resolved.merged, key)? {
        Some(value) => Ok(format!("{value}\n")),
        None => Ok(String::new()),
    }
}

fn config_set(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (project, rest) = split_project_flag(args);
    if rest.len() != 2 {
        return Err("usage: config set <key> <value> [--project]".to_string());
    }
    let path = target_path(workspace_start, project);
    settings::set_value(&path, &rest[0], &rest[1])?;
    Ok(format!(
        "Set {} = {} in {}.\n",
        rest[0],
        rest[1],
        path.display()
    ))
}

fn config_unset(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (project, rest) = split_project_flag(args);
    if rest.len() != 1 {
        return Err("usage: config unset <key> [--project]".to_string());
    }
    let path = target_path(workspace_start, project);
    let removed = settings::unset_value(&path, &rest[0])?;
    Ok(if removed {
        format!("Unset {} in {}.\n", rest[0], path.display())
    } else {
        format!("{} was not set.\n", rest[0])
    })
}

fn config_init(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (project, rest) = split_project_flag(args);
    let force = rest.iter().any(|arg| arg == "--force");
    let path = target_path(workspace_start, project);
    if path.exists() && !force {
        return Err(format!(
            "{} already exists (use --force to overwrite)",
            path.display()
        ));
    }
    settings::write_file_atomic(&path, &settings::starter_template())?;
    Ok(format!("Wrote {}.\n", path.display()))
}

fn config_edit(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (project, _) = split_project_flag(args);
    let path = target_path(workspace_start, project);
    if !path.exists() {
        settings::write_file_atomic(&path, &settings::starter_template())?;
    }
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|error| format!("failed to launch {editor}: {error}"))?;
    if !status.success() {
        return Err(format!("{editor} exited with status {status}"));
    }
    Ok(String::new())
}

/// `config profiles list` / `config profiles show <name>`. Profiles are the
/// `[agent.profiles.<name>]` bundles plus the built-in presets; a user profile
/// shadows a built-in of the same name.
fn config_profiles(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("list", &[][..]),
    };
    match command {
        "list" => config_profiles_list(workspace_start),
        "show" => config_profiles_show(workspace_start, rest),
        _ => Err(format!(
            "unknown config profiles command: {command} (try `list` or `show <name>`)"
        )),
    }
}

fn config_profiles_list(workspace_start: &Path) -> CliResult<String> {
    let resolved = settings::load_layered(workspace_start)?;
    let agent = resolved.agent();
    let builtin = settings::builtin_profiles();
    let mut output = String::from("agent profiles\n");
    if let Some(active) = &agent.profile {
        output.push_str(&format!("  active default (agent.profile): {active}\n"));
    }
    output.push('\n');
    for name in agent.available_profile_names() {
        // A user profile shadows a built-in of the same name.
        let (profile, origin): (&ProfileSettings, &str) = match agent.profiles.get(&name) {
            Some(profile) if builtin.contains_key(&name) => (profile, "user, shadows preset"),
            Some(profile) => (profile, "user"),
            None => (&builtin[&name], "preset"),
        };
        let description = profile.description.as_deref().unwrap_or("(no description)");
        output.push_str(&format!("  {name} [{origin}]\n"));
        output.push_str(&format!("    {description}\n"));
        let overrides = profile.overrides_summary();
        if !overrides.is_empty() {
            output.push_str(&format!("    overrides: {overrides}\n"));
        }
    }
    output.push_str("\nSelect with `--profile <name>` or `config set agent.profile <name>`.\n");
    Ok(output)
}

fn config_profiles_show(workspace_start: &Path, args: &[String]) -> CliResult<String> {
    let name = args
        .first()
        .ok_or_else(|| "usage: config profiles show <name>".to_string())?;
    let resolved = settings::load_layered(workspace_start)?;
    let agent = resolved.agent();
    // `resolve_profile` already errors with the available names if unknown.
    let profile = agent.resolve_profile(name)?;
    let origin = if agent.profiles.contains_key(name) {
        if settings::builtin_profiles().contains_key(name) {
            "user-defined (shadows built-in preset)"
        } else {
            "user-defined"
        }
    } else {
        "built-in preset"
    };
    let mut output = format!("profile {name} [{origin}]\n");
    if let Some(description) = &profile.description {
        output.push_str(&format!("  {description}\n"));
    }
    let overrides = profile.overrides_summary();
    if overrides.is_empty() {
        output.push_str("  (sets no overrides)\n");
    } else {
        output.push_str("  overrides:\n");
        for part in overrides.split(", ") {
            output.push_str(&format!("    {part}\n"));
        }
    }
    Ok(output)
}

fn config_reset(_args: &[String]) -> CliResult<String> {
    let path = settings::user_config_path();
    settings::write_file_atomic(&path, &settings::starter_template())?;
    Ok(format!("Reset {} to defaults.\n", path.display()))
}

fn config_migrate(_args: &[String]) -> CliResult<String> {
    let path = settings::user_config_path();
    let version = settings::migrate(&path)?;
    Ok(format!(
        "Config at {} is at version {version}.\n",
        path.display()
    ))
}
