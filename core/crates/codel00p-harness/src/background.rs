//! Background process management shared by the command tools.
//!
//! `run_command` with `background: true` spawns a long-running process (a dev
//! server, a file watcher, a test runner) without blocking the turn, and returns
//! a `process_id`. The model then polls incremental output with `process_output`,
//! sees what is still running with `process_list`, and stops a process with
//! `process_kill`. This mirrors the background-shell capability mature coding
//! agents rely on for anything that does not exit on its own.
//!
//! Output is drained continuously by reader threads (so a chatty process never
//! blocks on a full pipe) into per-stream buffers capped at [`MAX_STREAM_BYTES`];
//! `process_output` returns only the bytes appended since the previous read.

use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use crate::errors::HarnessError;

/// Per-stream buffer cap. Beyond this the oldest output is preserved and new
/// output is dropped (with a `truncated` flag), so a runaway process cannot
/// exhaust memory while still letting its pipe drain.
const MAX_STREAM_BYTES: usize = 256 * 1024;

/// A shared, cloneable handle to the set of background processes. Every command
/// tool in a registry shares one of these so they all see the same processes.
#[derive(Clone)]
pub struct BackgroundProcesses {
    inner: Arc<Inner>,
}

struct Inner {
    processes: Mutex<BTreeMap<String, Arc<ProcessEntry>>>,
    next_id: AtomicU64,
}

impl Default for BackgroundProcesses {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundProcesses {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                processes: Mutex::new(BTreeMap::new()),
                next_id: AtomicU64::new(1),
            }),
        }
    }

    /// Spawn a detached process and start draining its output. Returns the new
    /// process id.
    pub fn spawn(
        &self,
        program: &str,
        args: &[String],
        working_dir: &Path,
        label: String,
    ) -> Result<String, HarnessError> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| HarnessError::ToolFailed {
                name: "run_command".to_string(),
                message: error.to_string(),
            })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let entry = Arc::new(ProcessEntry {
            label,
            stdout: Arc::new(Mutex::new(StreamBuffer::default())),
            stderr: Arc::new(Mutex::new(StreamBuffer::default())),
            child: Mutex::new(child),
            finished: Mutex::new(None),
        });

        if let Some(stdout) = stdout {
            spawn_reader(stdout, entry.stdout.clone());
        }
        if let Some(stderr) = stderr {
            spawn_reader(stderr, entry.stderr.clone());
        }

        let id = format!("proc-{}", self.inner.next_id.fetch_add(1, Ordering::SeqCst));
        self.inner
            .processes
            .lock()
            .expect("processes lock")
            .insert(id.clone(), entry);
        Ok(id)
    }

    /// Read output appended since the previous read for `id`, advancing the read
    /// cursor, and report the process's current status. `None` if `id` is unknown.
    pub fn output(&self, id: &str, max_bytes: usize) -> Option<OutputSnapshot> {
        let entry = self.get(id)?;
        let status = entry.refresh_status();
        let (stdout, stdout_truncated) = entry.stdout.lock().expect("stdout").take_new(max_bytes);
        let (stderr, stderr_truncated) = entry.stderr.lock().expect("stderr").take_new(max_bytes);
        Some(OutputSnapshot {
            label: entry.label.clone(),
            status,
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        })
    }

    /// A snapshot of every known process and its status.
    pub fn list(&self) -> Vec<ProcessInfo> {
        let processes = self.inner.processes.lock().expect("processes lock");
        processes
            .iter()
            .map(|(id, entry)| ProcessInfo {
                id: id.clone(),
                label: entry.label.clone(),
                status: entry.refresh_status(),
            })
            .collect()
    }

    /// Kill a running process. Returns `None` if `id` is unknown, otherwise the
    /// post-kill status.
    pub fn kill(&self, id: &str) -> Option<ProcessStatus> {
        let entry = self.get(id)?;
        {
            let mut child = entry.child.lock().expect("child lock");
            let _ = child.kill();
            let _ = child.wait();
        }
        let mut finished = entry.finished.lock().expect("finished lock");
        if finished.is_none() {
            *finished = Some(ExitInfo {
                code: None,
                killed: true,
            });
        }
        Some(entry.status_from(&finished))
    }

    fn get(&self, id: &str) -> Option<Arc<ProcessEntry>> {
        self.inner
            .processes
            .lock()
            .expect("processes lock")
            .get(id)
            .cloned()
    }
}

struct ProcessEntry {
    label: String,
    stdout: Arc<Mutex<StreamBuffer>>,
    stderr: Arc<Mutex<StreamBuffer>>,
    child: Mutex<Child>,
    finished: Mutex<Option<ExitInfo>>,
}

impl ProcessEntry {
    /// Poll the child; if it has exited, record the exit info. Returns the
    /// current status.
    fn refresh_status(&self) -> ProcessStatus {
        let mut finished = self.finished.lock().expect("finished lock");
        if finished.is_none()
            && let Ok(Some(status)) = self.child.lock().expect("child lock").try_wait()
        {
            *finished = Some(ExitInfo {
                code: status.code(),
                killed: false,
            });
        }
        self.status_from(&finished)
    }

    fn status_from(&self, finished: &Option<ExitInfo>) -> ProcessStatus {
        match finished {
            None => ProcessStatus::Running,
            Some(info) => ProcessStatus::Exited {
                code: info.code,
                killed: info.killed,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct ExitInfo {
    code: Option<i32>,
    killed: bool,
}

/// Status of a background process at a point in time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Exited { code: Option<i32>, killed: bool },
}

/// A drained, byte-capped output buffer with a read cursor.
#[derive(Default)]
struct StreamBuffer {
    data: Vec<u8>,
    read_cursor: usize,
    truncated: bool,
}

impl StreamBuffer {
    fn append(&mut self, chunk: &[u8]) {
        let remaining = MAX_STREAM_BYTES.saturating_sub(self.data.len());
        if remaining == 0 {
            self.truncated = true;
            return;
        }
        if chunk.len() > remaining {
            self.data.extend_from_slice(&chunk[..remaining]);
            self.truncated = true;
        } else {
            self.data.extend_from_slice(chunk);
        }
    }

    /// Return the bytes appended since the last read (capped at `max_bytes`),
    /// advancing the cursor, plus whether the underlying buffer was truncated.
    fn take_new(&mut self, max_bytes: usize) -> (String, bool) {
        let end = self
            .read_cursor
            .saturating_add(max_bytes)
            .min(self.data.len());
        let slice = &self.data[self.read_cursor..end];
        let text = String::from_utf8_lossy(slice).to_string();
        self.read_cursor = end;
        (text, self.truncated)
    }
}

/// What [`BackgroundProcesses::output`] returns.
pub struct OutputSnapshot {
    pub label: String,
    pub status: ProcessStatus,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// What [`BackgroundProcesses::list`] returns per process.
pub struct ProcessInfo {
    pub id: String,
    pub label: String,
    pub status: ProcessStatus,
}

/// Spawn a thread that drains `reader` into `buffer` until EOF.
fn spawn_reader<R: Read + Send + 'static>(mut reader: R, buffer: Arc<Mutex<StreamBuffer>>) {
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buffer.lock().expect("stream buffer").append(&chunk[..n]),
            }
        }
    });
}
