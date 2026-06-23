//! Per-run debug log.
//!
//! Every run of the agent appends its full structured event stream — tool calls
//! (including the streamed raw arguments), inferences, permission decisions, and
//! failures — as JSON Lines to a file under `~/.codel00p/logs/`. A run that
//! misbehaves (a model that mis-shapes a tool call, a tool that fails in a loop)
//! can then be reproduced and debugged from the recorded trace instead of a
//! screenshot.
//!
//! Logging is best-effort and never affects a run: every I/O error is swallowed.
//! It is on by default; set `CODEL00P_LOG=off` (or `0`/`false`/`no`) to disable,
//! and `CODEL00P_LOG_DIR` to redirect the directory.

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process,
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use codel00p_harness::{AgentEventSink, HarnessEvent};

/// A process-wide handle to the current run's log file. Resolved once on first
/// use and shared by every harness built in the process, so all turns of a run
/// land in one file even when the harness is rebuilt per turn.
static RUN_LOG: OnceLock<Option<Arc<RunLog>>> = OnceLock::new();

/// An open per-run log file. Writes are serialized through a mutex so events
/// emitted concurrently (parallel tool calls) interleave as whole lines.
pub(crate) struct RunLog {
    path: PathBuf,
    file: Mutex<File>,
}

impl RunLog {
    fn append_line(&self, line: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Append one event as a JSON line. Serialization failures are dropped (the
    /// log is diagnostic, never load-bearing).
    fn record(&self, event: &HarnessEvent) {
        if let Ok(encoded) = serde_json::to_string(event) {
            self.append_line(&encoded);
        }
    }

    /// The on-disk path of this run's log, surfaced so callers can point a user
    /// at it.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// Print a one-line pointer to the run log on stderr, at most once per process.
/// Used by headless runs so the path is discoverable (mirroring the update
/// notice); the TUI must not have stderr written under its alternate screen, so
/// callers only invoke this on the non-TUI path.
pub(crate) fn announce(log: &RunLog) {
    static ANNOUNCED: OnceLock<()> = OnceLock::new();
    if ANNOUNCED.set(()).is_ok() {
        eprintln!("↳ run log: {}", log.path().display());
    }
}

/// The current run's log, or `None` when logging is disabled or the file could
/// not be opened. Computed once per process.
pub(crate) fn run_log() -> Option<Arc<RunLog>> {
    RUN_LOG.get_or_init(open_run_log).clone()
}

fn open_run_log() -> Option<Arc<RunLog>> {
    if logging_disabled() {
        return None;
    }
    let dir = log_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(run_file_name());
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    let log = Arc::new(RunLog {
        path,
        file: Mutex::new(file),
    });
    // A header line marks the start of a run with the binary version + pid, so a
    // directory of run logs stays self-describing and a tail of one file makes
    // clear which build produced it.
    log.append_line(&header_line());
    Some(log)
}

fn logging_disabled() -> bool {
    matches!(
        std::env::var("CODEL00P_LOG").ok().as_deref(),
        Some("off" | "0" | "false" | "no")
    )
}

fn log_dir() -> PathBuf {
    match std::env::var_os("CODEL00P_LOG_DIR").filter(|value| !value.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => crate::settings::home_dir().join("logs"),
    }
}

/// Epoch-second + pid filename, unique per process so each run gets its own file
/// without needing a date-formatting dependency.
fn run_file_name() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    format!("run-{epoch}-{}.jsonl", process::id())
}

fn header_line() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    serde_json::json!({
        "kind": "run_started",
        "version": crate::update::current_version(),
        "pid": process::id(),
        "epoch_secs": epoch,
    })
    .to_string()
}

/// An [`AgentEventSink`] that durably records every harness event to the run
/// log. Wired alongside any live sink so the trace is captured regardless of
/// whether a TUI or stdout consumer is attached.
pub(crate) struct FileEventSink {
    log: Arc<RunLog>,
}

impl FileEventSink {
    pub(crate) fn new(log: Arc<RunLog>) -> Self {
        Self { log }
    }
}

#[async_trait]
impl AgentEventSink for FileEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        self.log.record(event);
    }
}

/// Fans one harness event stream out to several sinks. The harness builder holds
/// a single event sink, so this composes durable file logging with a live TUI /
/// stdout sink without either displacing the other.
pub(crate) struct FanOutEventSink {
    sinks: Vec<Arc<dyn AgentEventSink>>,
}

impl FanOutEventSink {
    pub(crate) fn new(sinks: Vec<Arc<dyn AgentEventSink>>) -> Self {
        Self { sinks }
    }
}

#[async_trait]
impl AgentEventSink for FanOutEventSink {
    async fn emit(&self, event: &HarnessEvent) {
        for sink in &self.sinks {
            sink.emit(event).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_protocol::{EventId, SessionId, TurnId};

    fn tool_failed_event() -> HarnessEvent {
        HarnessEvent::ToolCallFailed {
            event_id: EventId::new(),
            session_id: SessionId::new(),
            turn_id: TurnId::new(),
            tool_name: "apply_patch".to_string(),
            message: "missing string field `path`".to_string(),
        }
    }

    #[tokio::test]
    async fn file_sink_appends_one_json_line_per_event() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.jsonl");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let log = Arc::new(RunLog {
            path: path.clone(),
            file: Mutex::new(file),
        });

        let sink = FileEventSink::new(log);
        sink.emit(&tool_failed_event()).await;
        sink.emit(&tool_failed_event()).await;

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "one line per event: {contents:?}");
        // Each line round-trips back into an event, and carries the failure
        // message we need to debug a mis-shaped tool call.
        for line in lines {
            let decoded: HarnessEvent = serde_json::from_str(line).unwrap();
            assert!(matches!(
                decoded,
                HarnessEvent::ToolCallFailed { ref message, .. } if message.contains("missing string field")
            ));
        }
    }

    #[tokio::test]
    async fn fan_out_delivers_to_every_sink() {
        let dir = tempfile::tempdir().unwrap();
        let open = |name: &str| {
            let path = dir.path().join(name);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            (
                path.clone(),
                Arc::new(FileEventSink::new(Arc::new(RunLog {
                    path,
                    file: Mutex::new(file),
                }))) as Arc<dyn AgentEventSink>,
            )
        };
        let (path_a, sink_a) = open("a.jsonl");
        let (path_b, sink_b) = open("b.jsonl");

        let fan = FanOutEventSink::new(vec![sink_a, sink_b]);
        fan.emit(&tool_failed_event()).await;

        assert_eq!(std::fs::read_to_string(&path_a).unwrap().lines().count(), 1);
        assert_eq!(std::fs::read_to_string(&path_b).unwrap().lines().count(), 1);
    }
}
