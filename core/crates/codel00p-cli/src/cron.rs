//! The `codel00p cron` command: define, run, and schedule jobs.
//!
//! Jobs are saved under `~/.codel00p/cron` as one TOML file each. `cron run`
//! executes a job once; `cron daemon` runs due jobs on a loop.

use std::{
    io::{self, Write},
    path::PathBuf,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codel00p_cron::{JobStore, due_jobs, parse_schedule};

use crate::{
    config::{CliConfig, CliResult},
    settings::{self, AgentSettings},
};

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cron_dir() -> PathBuf {
    settings::home_dir().join("cron")
}

fn store() -> JobStore {
    JobStore::new(cron_dir())
}

pub fn run(config: CliConfig, defaults: AgentSettings, args: &[String]) -> CliResult<String> {
    let (command, rest) = match args.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("list", &[][..]),
    };
    match command {
        "list" => cron_list(),
        "add" => cron_add(rest),
        "show" => cron_show(rest),
        "remove" | "rm" => cron_remove(rest),
        "enable" => cron_set_enabled(rest, true),
        "disable" => cron_set_enabled(rest, false),
        "run" => cron_run(config, &defaults, rest),
        "daemon" => cron_daemon(config, &defaults, rest),
        _ => Err(format!("unknown cron command: {command}")),
    }
}

fn cron_run(config: CliConfig, defaults: &AgentSettings, args: &[String]) -> CliResult<String> {
    let id = args.first().ok_or("usage: cron run <id>")?;
    let job = store().get(id).map_err(|error| error.to_string())?;
    if !job.enabled {
        return Err(format!("job {id} is disabled; enable it first"));
    }
    crate::agent::run_scheduled_job(config, defaults, &job)
}

struct DaemonOptions {
    interval_secs: u64,
    once: bool,
}

fn parse_daemon(args: &[String]) -> CliResult<DaemonOptions> {
    let mut interval_secs = 60;
    let mut once = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--once" => {
                once = true;
                index += 1;
            }
            "--interval" => {
                let value = value_after(args, index, "--interval")?;
                interval_secs = parse_schedule(&value)
                    .map(|schedule| schedule.interval_secs())
                    .map_err(|error| error.to_string())?;
                index += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown cron daemon option: {flag}"));
            }
            value => return Err(format!("unexpected argument: {value}")),
        }
    }
    Ok(DaemonOptions {
        interval_secs,
        once,
    })
}

fn cron_daemon(config: CliConfig, defaults: &AgentSettings, args: &[String]) -> CliResult<String> {
    let options = parse_daemon(args)?;
    if options.once {
        return Ok(tick_summary(&run_due_tick(&config, defaults)));
    }

    let mut stderr = io::stderr();
    writeln!(
        stderr,
        "codel00p cron daemon — checking every {}s (Ctrl-C to stop)",
        options.interval_secs
    )
    .ok();
    loop {
        let ran = run_due_tick(&config, defaults);
        if !ran.is_empty() {
            writeln!(stderr, "ran: {}", ran.join(", ")).ok();
        }
        thread::sleep(Duration::from_secs(options.interval_secs));
    }
}

/// Run every job due now. Marks each as run *before* executing so a slow run is
/// not double-fired by a later tick, and a failing run is logged but does not
/// stop the daemon. Returns the ids that were due.
fn run_due_tick(config: &CliConfig, defaults: &AgentSettings) -> Vec<String> {
    let now = now_epoch();
    let store = store();
    let jobs = store.list();
    let mut ran = Vec::new();
    for job in due_jobs(&jobs, now) {
        let _ = store.mark_ran(&job.id, now);
        if let Err(error) = crate::agent::run_scheduled_job(config.clone(), defaults, job) {
            eprintln!("cron {}: {error}", job.id);
        }
        ran.push(job.id.clone());
    }
    ran
}

fn tick_summary(ran: &[String]) -> String {
    if ran.is_empty() {
        "No jobs due.\n".to_string()
    } else {
        format!("Ran {} job(s): {}\n", ran.len(), ran.join(", "))
    }
}

fn cron_list() -> CliResult<String> {
    let jobs = store().list();
    if jobs.is_empty() {
        return Ok(
            "No scheduled jobs. Add one: codel00p cron add <schedule> <prompt>\n".to_string(),
        );
    }

    let mut output = String::from("Scheduled jobs:\n");
    for job in &jobs {
        let state = if job.enabled { " " } else { "off" };
        let schedule = parse_schedule(&job.schedule)
            .map(|s| s.describe())
            .unwrap_or_else(|_| format!("invalid:{}", job.schedule));
        output.push_str(&format!(
            "  [{state:<3}] {:<10} {:<14} {}\n",
            job.id,
            schedule,
            truncate(&job.prompt, 48)
        ));
    }
    output.push_str("\nShow:    codel00p cron show <id>\nRemove:  codel00p cron remove <id>\n");
    Ok(output)
}

struct AddOptions {
    schedule: String,
    prompt: String,
    workspace: Option<String>,
    provider: Option<String>,
    model: Option<String>,
}

fn parse_add(args: &[String]) -> CliResult<AddOptions> {
    let mut positional = Vec::new();
    let mut workspace = None;
    let mut provider = None;
    let mut model = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                workspace = Some(value_after(args, index, "--workspace")?);
                index += 2;
            }
            "--provider" => {
                provider = Some(value_after(args, index, "--provider")?);
                index += 2;
            }
            "--model" => {
                model = Some(value_after(args, index, "--model")?);
                index += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown cron add option: {flag}"));
            }
            value => {
                positional.push(value.to_string());
                index += 1;
            }
        }
    }

    let mut positional = positional.into_iter();
    let schedule = positional
        .next()
        .ok_or("usage: cron add <schedule> <prompt>")?;
    let prompt = positional.collect::<Vec<_>>().join(" ");
    if prompt.trim().is_empty() {
        return Err("usage: cron add <schedule> <prompt>".to_string());
    }
    Ok(AddOptions {
        schedule,
        prompt,
        workspace,
        provider,
        model,
    })
}

fn cron_add(args: &[String]) -> CliResult<String> {
    let options = parse_add(args)?;
    let job = store()
        .add(
            &options.schedule,
            &options.prompt,
            options.workspace,
            options.provider,
            options.model,
        )
        .map_err(|error| error.to_string())?;
    let schedule = parse_schedule(&job.schedule)
        .map(|s| s.describe())
        .unwrap_or_else(|_| job.schedule.clone());
    Ok(format!("Added {} ({schedule}).\n", job.id))
}

fn cron_show(args: &[String]) -> CliResult<String> {
    let id = args.first().ok_or("usage: cron show <id>")?;
    let job = store().get(id).map_err(|error| error.to_string())?;
    let schedule = parse_schedule(&job.schedule)
        .map(|s| s.describe())
        .unwrap_or_else(|_| job.schedule.clone());

    let mut output = format!("{}\n", job.id);
    output.push_str(&format!("  schedule:  {schedule} ({})\n", job.schedule));
    output.push_str(&format!(
        "  enabled:   {}\n",
        if job.enabled { "yes" } else { "no" }
    ));
    if let Some(workspace) = &job.workspace {
        output.push_str(&format!("  workspace: {workspace}\n"));
    }
    if let Some(provider) = &job.provider {
        output.push_str(&format!("  provider:  {provider}\n"));
    }
    if let Some(model) = &job.model {
        output.push_str(&format!("  model:     {model}\n"));
    }
    match job.last_run_epoch {
        Some(epoch) => output.push_str(&format!("  last run:  {epoch} (epoch seconds)\n")),
        None => output.push_str("  last run:  never\n"),
    }
    output.push_str(&format!("  prompt:    {}\n", job.prompt));
    Ok(output)
}

fn cron_remove(args: &[String]) -> CliResult<String> {
    let id = args.first().ok_or("usage: cron remove <id>")?;
    let removed = store().remove(id).map_err(|error| error.to_string())?;
    Ok(if removed {
        format!("Removed {id}.\n")
    } else {
        format!("No job named {id}.\n")
    })
}

fn cron_set_enabled(args: &[String], enabled: bool) -> CliResult<String> {
    let verb = if enabled { "enable" } else { "disable" };
    let id = args.first().ok_or(format!("usage: cron {verb} <id>"))?;
    let job = store()
        .set_enabled(id, enabled)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "{} {}.\n",
        if job.enabled { "Enabled" } else { "Disabled" },
        job.id
    ))
}

fn value_after(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

fn truncate(text: &str, max: usize) -> String {
    let oneline = text.replace('\n', " ");
    if oneline.chars().count() > max {
        format!("{}...", oneline.chars().take(max - 3).collect::<String>())
    } else {
        oneline
    }
}
