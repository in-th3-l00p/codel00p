//! The full-screen `codel00p cron` dialog.
//!
//! Bare `codel00p cron` opens this on a terminal; the scriptable subcommands
//! (`list`, `add`, `run`, ...) remain for non-TTY use. The pure model and rendering
//! live in [`model`] and [`view`]; the terminal lifecycle and loop are shared via
//! [`crate::dialog`], and the closure here performs the store/agent effects an
//! update asks for — reusing the exact operations behind the subcommands.

mod model;
#[cfg(test)]
mod tests;
mod view;

use codel00p_cron::{CronJob, JobStore, parse_schedule};

use crate::config::{CliConfig, CliResult};
use crate::settings::{self, AgentSettings};
use model::{CronModel, CronRow, Flow, JobDetail, Mutation};

fn store() -> JobStore {
    JobStore::new(settings::home_dir().join("cron"))
}

/// Runs the cron dialog. The terminal lifecycle and blocking loop come from
/// [`crate::dialog`]; the `on_key` closure performs the store and agent effects.
pub(crate) fn run(config: CliConfig, defaults: AgentSettings) -> CliResult<String> {
    let store = store();
    let mut model = CronModel::new();
    reload(&store, &mut model);

    crate::dialog::run_blocking(&mut model, view::draw, |model, key| {
        match model.update(key) {
            Flow::Stay => {}
            Flow::Quit => return Ok(false),
            Flow::OpenDetail(id) => open_detail(&store, model, &id),
            Flow::RunNow(id) => {
                run_now(&store, &config, &defaults, model, &id);
            }
            Flow::Mutate(mutation) => {
                apply(&store, model, mutation);
                model.screen = model::Screen::List;
                reload(&store, model);
            }
        }
        Ok(true)
    })?;
    Ok("Closed cron dialog.\n".to_string())
}

/// Re-lists jobs and feeds them to the model.
fn reload(store: &JobStore, model: &mut CronModel) {
    model.set_rows(store.list().iter().map(row_from_job).collect());
}

/// Opens the detail screen for the selected job.
fn open_detail(store: &JobStore, model: &mut CronModel, id: &str) {
    match store.get(id) {
        Ok(job) => model.show_detail(row_from_job(&job)),
        Err(error) => model.set_status(error.to_string()),
    }
}

/// Runs a job now, mirroring `cron run`: it must be enabled, then the same
/// executor the subcommand uses runs it.
fn run_now(
    store: &JobStore,
    config: &CliConfig,
    defaults: &AgentSettings,
    model: &mut CronModel,
    id: &str,
) {
    let job = match store.get(id) {
        Ok(job) => job,
        Err(error) => {
            model.set_status(error.to_string());
            return;
        }
    };
    if !job.enabled {
        model.set_status(format!("{id} is disabled; enable it first."));
        return;
    }
    match crate::cron::execute_job(config.clone(), defaults, &job) {
        Ok(_) => model.set_status(format!("Ran {id}.")),
        Err(error) => model.set_status(format!("{id}: {error}")),
    }
}

/// Applies a store effect, recording success or the error in the status line.
fn apply(store: &JobStore, model: &mut CronModel, mutation: Mutation) {
    let outcome = match mutation {
        Mutation::SetEnabled { id, enabled } => store.set_enabled(&id, enabled).map(|job| {
            format!(
                "{} {}.",
                if job.enabled { "Enabled" } else { "Disabled" },
                job.id
            )
        }),
        Mutation::Delete { id } => store.remove(&id).map(|removed| {
            if removed {
                format!("Removed {id}.")
            } else {
                format!("No job named {id}.")
            }
        }),
        Mutation::Add { schedule, prompt } => store
            .add(&schedule, &prompt, None, None, None)
            .map(|job| format!("Added {} ({}).", job.id, describe(&job.schedule))),
    };
    match outcome {
        Ok(message) => model.set_status(message),
        Err(error) => model.set_status(error.to_string()),
    }
}

fn row_from_job(job: &CronJob) -> CronRow {
    CronRow {
        id: job.id.clone(),
        schedule: describe(&job.schedule),
        enabled: job.enabled,
        action: job.action(),
        detail: JobDetail {
            schedule_spec: job.schedule.clone(),
            prompt: job.prompt.clone(),
            command: job.command.clone(),
            workspace: job.workspace.clone(),
            provider: job.provider.clone(),
            model: job.model.clone(),
            last_run_epoch: job.last_run_epoch,
        },
    }
}

/// Human-readable schedule, falling back to `invalid:<spec>` like `cron list`.
fn describe(spec: &str) -> String {
    parse_schedule(spec)
        .map(|schedule| schedule.describe())
        .unwrap_or_else(|_| format!("invalid:{spec}"))
}
