//! Execution-backend seam for command tools.
//!
//! All command execution (`run_command` and the `process_*` tools, via
//! [`crate::background::BackgroundProcesses`]) routes through a
//! [`TerminalBackend`] rather than calling `std::process::Command` directly.
//! This is the abstraction that lets a future Docker / SSH / cloud backend run
//! the same commands somewhere other than the bare local workspace without
//! touching the tool or agent contracts.
//!
//! Phase 1 of initiative #7 ships only [`LocalBackend`], which is the existing
//! local-execution logic moved behind the trait verbatim (same Stdio wiring,
//! same timeout poll interval, same kill-on-timeout, same output capping). It is
//! a refactor with no behavior change.
//!
//! The trait is expressed in plain data ([`CommandSpec`], [`CommandOutcome`],
//! [`OutputLimits`]) rather than `std::process` types so backends that do not
//! shell out locally are implementable. The one concession to "a child process
//! somewhere" is [`ChildHandle`], which exposes piped stdout/stderr plus
//! wait/kill — exactly what [`BackgroundProcesses`](crate::background) needs to
//! keep its reader-thread + `StreamBuffer` draining intact.

use std::{
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use crate::errors::HarnessError;

mod docker;

pub use docker::{DockerBackend, DockerConfig};

const POLL_INTERVAL_MS: u64 = 10;

/// A command to execute, expressed in data so any backend can interpret it.
///
/// `cwd` is the already-resolved, workspace-bounded working directory (the tool
/// resolves it via [`Workspace::resolve_directory`](crate::workspace::Workspace)
/// before handing it to the backend).
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>, cwd: PathBuf) -> Self {
        Self {
            program: program.into(),
            args,
            cwd,
        }
    }
}

/// Time and output-size limits applied to a foreground run.
#[derive(Clone, Copy, Debug)]
pub struct OutputLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

/// The structured result of a foreground run — everything `RunCommandTool`
/// reports to the model. Output is already UTF-8-lossy-decoded and capped to
/// `max_output_bytes`, with `*_truncated` flags recording whether anything was
/// dropped.
#[derive(Clone, Debug)]
pub struct CommandOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// A handle to a spawned background process, abstracting the child so a backend
/// other than local can satisfy it. The piped streams are taken once each (the
/// background store hands them to reader threads); `try_wait` / `wait` / `kill`
/// mirror `std::process::Child`.
///
/// The handle is `Send` so it can live in the background-process store and be
/// killed/waited from any thread.
pub trait ChildHandle: Send {
    /// Take the child's stdout pipe, if any (callable once).
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>>;
    /// Take the child's stderr pipe, if any (callable once).
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>>;
    /// Non-blocking exit check. `Ok(Some(code))` when the child has exited
    /// (`code` is its exit code, if available); `Ok(None)` while still running.
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, HarnessError>;
    /// Block until the child exits.
    fn wait(&mut self) -> Result<Option<i32>, HarnessError>;
    /// Request termination. Best-effort; pair with [`ChildHandle::wait`].
    fn kill(&mut self) -> Result<(), HarnessError>;
}

/// Where command tools run programs. Object-safe and `Send + Sync` so it can be
/// stored as `Arc<dyn TerminalBackend>` and shared across the async command
/// tools.
pub trait TerminalBackend: Send + Sync {
    /// Run a command to completion (or until `limits.timeout`), returning a
    /// structured outcome with capped output.
    fn run_foreground(
        &self,
        spec: &CommandSpec,
        limits: OutputLimits,
    ) -> Result<CommandOutcome, HarnessError>;

    /// Spawn a command without waiting, returning a handle the background store
    /// manages (drains output, polls status, kills).
    fn spawn_background(&self, spec: &CommandSpec) -> Result<Box<dyn ChildHandle>, HarnessError>;

    /// Whether this backend isolates command execution from the host (container,
    /// VM, remote sandbox) rather than running directly on the local machine.
    ///
    /// Governance layers use this to enforce policies such as "unattended or
    /// gateway-initiated shell must run in an isolating backend." The default is
    /// `false` (fail-closed: an unknown backend is treated as non-isolating), so
    /// only a backend that genuinely sandboxes execution should override it.
    fn is_isolated(&self) -> bool {
        false
    }
}

/// The default backend: runs commands locally, exactly as before the seam
/// existed. Stateless and cheap to clone behind an `Arc`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalBackend;

impl LocalBackend {
    pub fn new() -> Self {
        Self
    }
}

impl TerminalBackend for LocalBackend {
    fn run_foreground(
        &self,
        spec: &CommandSpec,
        limits: OutputLimits,
    ) -> Result<CommandOutcome, HarnessError> {
        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| HarnessError::ToolFailed {
                name: "run_command".to_string(),
                message: error.to_string(),
            })?;

        let started = Instant::now();
        let mut timed_out = false;
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if started.elapsed() >= limits.timeout {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        let output = child.wait_with_output()?;
        let stdout = cap_output(&output.stdout, limits.max_output_bytes);
        let stderr = cap_output(&output.stderr, limits.max_output_bytes);
        let exit_code = if timed_out {
            None
        } else {
            output.status.code()
        };
        let success = !timed_out && output.status.success();

        Ok(CommandOutcome {
            exit_code,
            success,
            timed_out,
            stdout: stdout.content,
            stderr: stderr.content,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
        })
    }

    fn spawn_background(&self, spec: &CommandSpec) -> Result<Box<dyn ChildHandle>, HarnessError> {
        let child = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| HarnessError::ToolFailed {
                name: "run_command".to_string(),
                message: error.to_string(),
            })?;
        Ok(Box::new(LocalChildHandle { child }))
    }
}

/// A `ChildHandle` backed by a local `std::process::Child`.
struct LocalChildHandle {
    child: Child,
}

impl ChildHandle for LocalChildHandle {
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.child
            .stdout
            .take()
            .map(|stdout| Box::new(stdout) as Box<dyn Read + Send>)
    }

    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.child
            .stderr
            .take()
            .map(|stderr| Box::new(stderr) as Box<dyn Read + Send>)
    }

    fn try_wait(&mut self) -> Result<Option<Option<i32>>, HarnessError> {
        Ok(self.child.try_wait()?.map(|status| status.code()))
    }

    fn wait(&mut self) -> Result<Option<i32>, HarnessError> {
        Ok(self.child.wait()?.code())
    }

    fn kill(&mut self) -> Result<(), HarnessError> {
        let _ = self.child.kill();
        Ok(())
    }
}

struct CappedOutput {
    content: String,
    truncated: bool,
}

/// Decode `bytes` lossily, capping to `max_bytes` and flagging truncation.
fn cap_output(bytes: &[u8], max_bytes: usize) -> CappedOutput {
    let truncated = bytes.len() > max_bytes;
    let end = bytes.len().min(max_bytes);
    CappedOutput {
        content: String::from_utf8_lossy(&bytes[..end]).to_string(),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> CommandSpec {
        CommandSpec::new(
            "sh",
            args.iter().map(|s| s.to_string()).collect(),
            std::env::temp_dir(),
        )
    }

    fn limits() -> OutputLimits {
        OutputLimits {
            timeout: Duration::from_secs(10),
            max_output_bytes: 16_384,
        }
    }

    #[test]
    fn foreground_success_captures_stdout() {
        let backend = LocalBackend::new();
        let outcome = backend
            .run_foreground(&spec(&["-c", "printf hi; printf err 1>&2"]), limits())
            .unwrap();
        assert!(outcome.success);
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout, "hi");
        assert_eq!(outcome.stderr, "err");
        assert!(!outcome.stdout_truncated);
    }

    #[test]
    fn foreground_nonzero_exit_is_not_success() {
        let backend = LocalBackend::new();
        let outcome = backend
            .run_foreground(&spec(&["-c", "exit 3"]), limits())
            .unwrap();
        assert!(!outcome.success);
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, Some(3));
    }

    #[test]
    fn foreground_timeout_reports_timed_out_with_no_code() {
        let backend = LocalBackend::new();
        let outcome = backend
            .run_foreground(
                &spec(&["-c", "sleep 5"]),
                OutputLimits {
                    timeout: Duration::from_millis(100),
                    max_output_bytes: 16_384,
                },
            )
            .unwrap();
        assert!(outcome.timed_out);
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, None);
    }

    #[test]
    fn foreground_output_is_capped() {
        let backend = LocalBackend::new();
        let outcome = backend
            .run_foreground(
                &spec(&["-c", "printf '%s' aaaaaaaaaa"]),
                OutputLimits {
                    timeout: Duration::from_secs(10),
                    max_output_bytes: 4,
                },
            )
            .unwrap();
        assert_eq!(outcome.stdout, "aaaa");
        assert!(outcome.stdout_truncated);
    }

    #[test]
    fn background_spawn_output_and_kill() {
        let backend = LocalBackend::new();

        // Spawn a short-lived process, drain its stdout, and confirm exit code.
        let mut handle = backend
            .spawn_background(&spec(&["-c", "printf done"]))
            .unwrap();
        let mut stdout = handle.take_stdout().expect("stdout pipe");
        let mut buf = String::new();
        stdout.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "done");
        let code = handle.wait().unwrap();
        assert_eq!(code, Some(0));

        // Spawn a long-lived process and kill it.
        let mut handle = backend
            .spawn_background(&spec(&["-c", "sleep 30"]))
            .unwrap();
        assert!(handle.try_wait().unwrap().is_none());
        handle.kill().unwrap();
        let _ = handle.wait().unwrap();
    }
}
