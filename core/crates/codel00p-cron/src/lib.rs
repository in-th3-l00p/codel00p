//! Scheduling primitives: parse schedule specs and persist cron jobs.
//!
//! This is the foundation of the [Scheduling initiative](../../../docs/initiatives/scheduling-cron.md),
//! Phase 1. It models a [`CronJob`] (a saved prompt + schedule), parses
//! duration-interval [`Schedule`] specs, and stores jobs as TOML files via
//! [`JobStore`]. Computing the next run is deterministic (`now` is passed in) so
//! it is testable; a daemon and the agent executor build on this in later
//! slices.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CronError {
    #[error("invalid schedule `{spec}`: {reason}")]
    InvalidSchedule { spec: String, reason: String },
    #[error("invalid job name `{id}`")]
    InvalidId { id: String },
    #[error("no job named `{id}`")]
    UnknownJob { id: String },
    #[error("a prompt is required")]
    EmptyPrompt,
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

/// A recurring interval, parsed from a spec like `30m`, `2h`, `1d`, `every 1w`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    interval_secs: u64,
}

impl Schedule {
    pub fn interval_secs(self) -> u64 {
        self.interval_secs
    }

    /// The next run time at or after `now` (epoch seconds). For an interval
    /// schedule this is simply `now + interval`.
    pub fn next_after(self, now_epoch_secs: u64) -> u64 {
        now_epoch_secs.saturating_add(self.interval_secs)
    }

    /// A compact human description, e.g. `every 30m`.
    pub fn describe(self) -> String {
        format!("every {}", humanize_secs(self.interval_secs))
    }
}

/// Parse a schedule spec. Supports duration intervals (`<n><unit>` where unit is
/// `s`, `m`, `h`, `d`, or `w`), optionally prefixed with `every `. Multi-field
/// cron expressions are a later slice.
pub fn parse_schedule(spec: &str) -> Result<Schedule, CronError> {
    let trimmed = spec.trim();
    let body = trimmed
        .strip_prefix("every ")
        .or_else(|| trimmed.strip_prefix("every"))
        .unwrap_or(trimmed)
        .trim();

    let invalid = |reason: &str| CronError::InvalidSchedule {
        spec: spec.to_string(),
        reason: reason.to_string(),
    };

    let split = body
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| invalid("expected a number and a unit, e.g. 30m"))?;
    let (number, unit) = body.split_at(split);
    let amount: u64 = number
        .trim()
        .parse()
        .map_err(|_| invalid("expected a leading number, e.g. 30m"))?;
    if amount == 0 {
        return Err(invalid("interval must be greater than zero"));
    }
    let unit_secs = match unit.trim() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
        "d" | "day" | "days" => 86_400,
        "w" | "week" | "weeks" => 604_800,
        other => return Err(invalid(&format!("unknown unit `{other}`"))),
    };

    Ok(Schedule {
        interval_secs: amount * unit_secs,
    })
}

/// A saved scheduled job: a prompt to run on a schedule, plus optional run
/// overrides. Persisted as one TOML file per job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    /// The raw schedule spec (e.g. `30m`); parse with [`parse_schedule`].
    pub schedule: String,
    pub prompt: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Epoch seconds of the last run attempt, or `None` if it has never run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_epoch: Option<u64>,
}

impl CronJob {
    /// The parsed schedule, or an error if the stored spec is invalid.
    pub fn parsed_schedule(&self) -> Result<Schedule, CronError> {
        parse_schedule(&self.schedule)
    }

    /// Whether this job should run at `now` (epoch seconds): it must be enabled,
    /// have a valid schedule, and either have never run or be past its next-run
    /// time. A never-run job is due on the first check.
    pub fn is_due(&self, now: u64) -> bool {
        if !self.enabled {
            return false;
        }
        let Ok(schedule) = self.parsed_schedule() else {
            return false;
        };
        match self.last_run_epoch {
            None => true,
            Some(last) => schedule.next_after(last) <= now,
        }
    }
}

/// The enabled jobs in `jobs` that are due to run at `now`.
pub fn due_jobs(jobs: &[CronJob], now: u64) -> Vec<&CronJob> {
    jobs.iter().filter(|job| job.is_due(now)).collect()
}

fn default_true() -> bool {
    true
}

/// File-backed store of cron jobs: one `<id>.toml` per job under a directory.
pub struct JobStore {
    dir: PathBuf,
}

impl JobStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Save a new job, assigning the next `cron-N` id. Validates the schedule and
    /// a non-empty prompt up front.
    pub fn add(
        &self,
        schedule: &str,
        prompt: &str,
        workspace: Option<String>,
        provider: Option<String>,
        model: Option<String>,
    ) -> Result<CronJob, CronError> {
        parse_schedule(schedule)?;
        if prompt.trim().is_empty() {
            return Err(CronError::EmptyPrompt);
        }

        let id = format!("cron-{}", self.next_index());
        let job = CronJob {
            id: id.clone(),
            schedule: schedule.trim().to_string(),
            prompt: prompt.trim().to_string(),
            enabled: true,
            workspace,
            provider,
            model,
            last_run_epoch: None,
        };
        self.write(&job)?;
        Ok(job)
    }

    /// All saved jobs, sorted by id.
    pub fn list(&self) -> Vec<CronJob> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut jobs: Vec<CronJob> = entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|entry| self.read(&entry.path()).ok())
            .collect();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        jobs
    }

    pub fn get(&self, id: &str) -> Result<CronJob, CronError> {
        validate_id(id)?;
        let path = self.job_path(id);
        if !path.is_file() {
            return Err(CronError::UnknownJob { id: id.to_string() });
        }
        self.read(&path)
    }

    /// Remove a job. Returns whether one was removed.
    pub fn remove(&self, id: &str) -> Result<bool, CronError> {
        validate_id(id)?;
        let path = self.job_path(id);
        if !path.is_file() {
            return Ok(false);
        }
        fs::remove_file(&path).map_err(|source| CronError::Io { path, source })?;
        Ok(true)
    }

    /// Enable or disable a job, returning the updated record.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<CronJob, CronError> {
        let mut job = self.get(id)?;
        job.enabled = enabled;
        self.write(&job)?;
        Ok(job)
    }

    /// Record that a job ran at `ran_at_epoch`, advancing when it is next due.
    pub fn mark_ran(&self, id: &str, ran_at_epoch: u64) -> Result<CronJob, CronError> {
        let mut job = self.get(id)?;
        job.last_run_epoch = Some(ran_at_epoch);
        self.write(&job)?;
        Ok(job)
    }

    fn next_index(&self) -> u64 {
        self.list()
            .iter()
            .filter_map(|job| job.id.strip_prefix("cron-"))
            .filter_map(|n| n.parse::<u64>().ok())
            .max()
            .map(|max| max + 1)
            .unwrap_or(1)
    }

    fn job_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    fn write(&self, job: &CronJob) -> Result<(), CronError> {
        fs::create_dir_all(&self.dir).map_err(|source| CronError::Io {
            path: self.dir.clone(),
            source,
        })?;
        let contents = toml::to_string_pretty(job).expect("serialize cron job");
        let path = self.job_path(&job.id);
        fs::write(&path, contents).map_err(|source| CronError::Io { path, source })
    }

    fn read(&self, path: &Path) -> Result<CronJob, CronError> {
        let text = fs::read_to_string(path).map_err(|source| CronError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| CronError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }
}

fn validate_id(id: &str) -> Result<(), CronError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(CronError::InvalidId { id: id.to_string() })
    }
}

fn humanize_secs(secs: u64) -> String {
    for (unit_secs, suffix) in [(604_800, "w"), (86_400, "d"), (3_600, "h"), (60, "m")] {
        if secs >= unit_secs && secs.is_multiple_of(unit_secs) {
            return format!("{}{suffix}", secs / unit_secs);
        }
    }
    format!("{secs}s")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_duration_specs() {
        assert_eq!(parse_schedule("30m").unwrap().interval_secs(), 1_800);
        assert_eq!(parse_schedule("2h").unwrap().interval_secs(), 7_200);
        assert_eq!(parse_schedule("1d").unwrap().interval_secs(), 86_400);
        assert_eq!(parse_schedule("every 1w").unwrap().interval_secs(), 604_800);
        assert_eq!(
            parse_schedule("  45 minutes ").unwrap().interval_secs(),
            2_700
        );
    }

    #[test]
    fn rejects_bad_specs() {
        assert!(parse_schedule("").is_err());
        assert!(parse_schedule("soon").is_err());
        assert!(parse_schedule("0m").is_err());
        assert!(parse_schedule("10y").is_err());
    }

    #[test]
    fn next_after_adds_the_interval() {
        let schedule = parse_schedule("30m").unwrap();
        assert_eq!(schedule.next_after(1000), 1000 + 1800);
        assert_eq!(schedule.describe(), "every 30m");
    }

    #[test]
    fn store_round_trips_jobs() {
        let dir = tempdir().unwrap();
        let store = JobStore::new(dir.path());

        let job = store
            .add("1h", "Run the nightly checks", None, None, None)
            .unwrap();
        assert_eq!(job.id, "cron-1");
        assert!(job.enabled);

        let second = store.add("30m", "Sync memory", None, None, None).unwrap();
        assert_eq!(second.id, "cron-2");

        let listed = store.list();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, "cron-1");

        assert_eq!(
            store.get("cron-1").unwrap().prompt,
            "Run the nightly checks"
        );
        assert!(!store.set_enabled("cron-1", false).unwrap().enabled);
        assert!(!store.get("cron-1").unwrap().enabled);

        assert!(store.remove("cron-1").unwrap());
        assert!(!store.remove("cron-1").unwrap());
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn due_detection_tracks_run_state() {
        let dir = tempdir().unwrap();
        let store = JobStore::new(dir.path());
        let job = store.add("1h", "nightly", None, None, None).unwrap();

        // Never run -> due now.
        assert!(job.is_due(10_000));

        // After running, not due again until an interval has passed.
        let ran = store.mark_ran(&job.id, 10_000).unwrap();
        assert_eq!(ran.last_run_epoch, Some(10_000));
        assert!(!ran.is_due(10_000));
        assert!(!ran.is_due(10_000 + 3_599));
        assert!(ran.is_due(10_000 + 3_600));

        // Disabled jobs are never due.
        let off = store.set_enabled(&job.id, false).unwrap();
        assert!(!off.is_due(10_000 + 100_000));
    }

    #[test]
    fn due_jobs_filters_to_runnable() {
        let dir = tempdir().unwrap();
        let store = JobStore::new(dir.path());
        store.add("1h", "a", None, None, None).unwrap();
        let b = store.add("1h", "b", None, None, None).unwrap();
        store.mark_ran(&b.id, 10_000).unwrap();

        let jobs = store.list();
        let due = due_jobs(&jobs, 10_000);
        // a is never-run (due); b just ran (not due).
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].prompt, "a");
    }

    #[test]
    fn add_validates_schedule_and_prompt() {
        let dir = tempdir().unwrap();
        let store = JobStore::new(dir.path());
        assert!(matches!(
            store.add("nope", "x", None, None, None),
            Err(CronError::InvalidSchedule { .. })
        ));
        assert!(matches!(
            store.add("1h", "  ", None, None, None),
            Err(CronError::EmptyPrompt)
        ));
    }
}
