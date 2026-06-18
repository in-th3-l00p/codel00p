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
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use crate::{
    errors::HarnessError,
    terminal::{ChildHandle, CommandSpec, LocalBackend, TerminalBackend},
};

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
    /// Where commands are actually executed. The store spawns through this
    /// backend instead of calling `Command::new` itself, so the same background
    /// machinery (reader threads + `StreamBuffer` + join-before-exited) works
    /// for any backend.
    backend: Arc<dyn TerminalBackend>,
}

impl Default for BackgroundProcesses {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundProcesses {
    pub fn new() -> Self {
        Self::with_backend(Arc::new(LocalBackend::new()))
    }

    /// Construct a store that spawns through `backend`. `new()` uses
    /// [`LocalBackend`], preserving today's behavior exactly.
    pub fn with_backend(backend: Arc<dyn TerminalBackend>) -> Self {
        Self {
            inner: Arc::new(Inner {
                processes: Mutex::new(BTreeMap::new()),
                next_id: AtomicU64::new(1),
                backend,
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
        let spec = CommandSpec::new(program, args.to_vec(), working_dir.to_path_buf());
        let mut child = self.inner.backend.spawn_background(&spec)?;

        let stdout = child.take_stdout();
        let stderr = child.take_stderr();
        let stdout_buffer = Arc::new(Mutex::new(StreamBuffer::default()));
        let stderr_buffer = Arc::new(Mutex::new(StreamBuffer::default()));

        // Keep the reader-thread handles so finalizing an exited process can join
        // them, guaranteeing every byte the child wrote has landed in the buffers
        // before we ever report `exited` to a consumer of `output`.
        let mut readers = Vec::new();
        if let Some(stdout) = stdout {
            readers.push(spawn_reader(stdout, stdout_buffer.clone()));
        }
        if let Some(stderr) = stderr {
            readers.push(spawn_reader(stderr, stderr_buffer.clone()));
        }

        let entry = Arc::new(ProcessEntry {
            label,
            stdout: stdout_buffer,
            stderr: stderr_buffer,
            child: Mutex::new(child),
            finished: Mutex::new(None),
            readers: Mutex::new(readers),
        });

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
        // The child is gone, so its pipe write ends are closed; join the readers
        // so a post-kill `output` read sees everything it managed to write.
        entry.drain_readers();
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
    child: Mutex<Box<dyn ChildHandle>>,
    finished: Mutex<Option<ExitInfo>>,
    readers: Mutex<Vec<thread::JoinHandle<()>>>,
}

impl ProcessEntry {
    /// Poll the child; if it has exited, record the exit info. Returns the
    /// current status.
    ///
    /// On the transition to exited we join the reader threads. Once the child
    /// has exited its stdout/stderr pipe write ends are closed, so each reader
    /// is guaranteed to hit EOF and terminate promptly; joining them before we
    /// publish the `exited` status makes the contract "exited ⇒ all output is in
    /// the buffers" real for every consumer of `output`, eliminating the race
    /// where `try_wait` observes exit before the readers copied the final bytes.
    fn refresh_status(&self) -> ProcessStatus {
        let mut finished = self.finished.lock().expect("finished lock");
        if finished.is_none()
            && let Ok(Some(code)) = self.child.lock().expect("child lock").try_wait()
        {
            self.drain_readers();
            *finished = Some(ExitInfo {
                code,
                killed: false,
            });
        }
        self.status_from(&finished)
    }

    /// Join the reader threads, draining each pipe to EOF. Safe to call more than
    /// once: after the first call the handle vector is empty.
    fn drain_readers(&self) {
        let handles: Vec<_> = self
            .readers
            .lock()
            .expect("readers lock")
            .drain(..)
            .collect();
        for handle in handles {
            let _ = handle.join();
        }
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

/// Spawn a thread that drains `reader` into `buffer` until EOF, returning its
/// join handle so the process can wait for the pipe to fully drain on exit.
fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    buffer: Arc<Mutex<StreamBuffer>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buffer.lock().expect("stream buffer").append(&chunk[..n]),
            }
        }
    })
}
