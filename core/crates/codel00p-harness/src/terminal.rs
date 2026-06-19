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
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use crate::errors::HarnessError;

mod docker;
mod local_fs;

pub use docker::{DockerBackend, DockerConfig};
pub use local_fs::{DirEntry, FileKind};

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

    // --- Filesystem ops (initiative #7, phase 3) -------------------------
    //
    // All paths are WORKSPACE-RELATIVE. The backend roots them at its own
    // workspace and is responsible for keeping the resolved real path inside
    // that workspace (the same canonicalize + `starts_with(root)` boundary
    // `Workspace` historically enforced). The default impls return a clear
    // error so a backend that does not implement a workspace filesystem (the
    // fallback) fails loudly rather than touching the host fs; `LocalBackend`
    // and `DockerBackend` override all of them via the shared local-fs impl.

    /// Read a workspace-relative file's raw bytes.
    fn read_file(&self, rel: &Path) -> Result<Vec<u8>, HarnessError> {
        Err(unsupported_fs(rel))
    }

    /// Write `contents` to a workspace-relative path, creating parent
    /// directories as needed.
    fn write_file(&self, rel: &Path, _contents: &[u8]) -> Result<(), HarnessError> {
        Err(unsupported_fs(rel))
    }

    /// Delete a workspace-relative file.
    fn delete_file(&self, rel: &Path) -> Result<(), HarnessError> {
        Err(unsupported_fs(rel))
    }

    /// Recursively create a workspace-relative directory.
    fn create_dir_all(&self, rel: &Path) -> Result<(), HarnessError> {
        Err(unsupported_fs(rel))
    }

    /// Report whether a workspace-relative path exists and what it is.
    fn metadata(&self, rel: &Path) -> Result<FileKind, HarnessError> {
        Err(unsupported_fs(rel))
    }

    /// List the immediate entries (name + is_dir) of a workspace-relative
    /// directory. Wired by the traversal tools in PR2 of initiative #7.
    fn read_dir(&self, rel: &Path) -> Result<Vec<DirEntry>, HarnessError> {
        Err(unsupported_fs(rel))
    }

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

/// Error returned by the default (no-op) filesystem trait methods so an
/// fs-incapable backend fails loudly instead of silently touching the host.
fn unsupported_fs(rel: &Path) -> HarnessError {
    HarnessError::ToolFailed {
        name: "workspace".to_string(),
        message: format!(
            "this execution backend does not support workspace filesystem access ({})",
            rel.display()
        ),
    }
}

/// The default backend: runs commands locally, exactly as before the seam
/// existed. Cheap to clone behind an `Arc`.
///
/// Command execution is stateless (cwd is an absolute path in [`CommandSpec`]),
/// but the workspace filesystem ops need to know where the workspace lives, so
/// the backend optionally carries the workspace root. Construct with
/// [`LocalBackend::rooted`] to enable fs ops; [`LocalBackend::new`] keeps the
/// historical stateless behavior for command-only call sites (the fs ops then
/// error like any fs-incapable backend).
#[derive(Clone, Debug, Default)]
pub struct LocalBackend {
    /// Absolute workspace root for filesystem ops, if this backend is rooted.
    workspace_root: Option<PathBuf>,
}

impl LocalBackend {
    /// A stateless local backend for command execution only. Filesystem ops on
    /// it return an "unsupported" error (no root to bound them); use
    /// [`LocalBackend::rooted`] when the backend also serves workspace fs ops.
    pub fn new() -> Self {
        Self {
            workspace_root: None,
        }
    }

    /// A local backend whose filesystem ops are rooted at (and bounded to)
    /// `workspace_root`, which must be an absolute, canonicalized host path.
    pub fn rooted(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: Some(workspace_root.into()),
        }
    }

    /// The workspace root this backend's fs ops are bounded to, if any.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// The workspace root, or an "unsupported" error if this backend is not
    /// rooted (constructed via [`LocalBackend::new`]).
    fn root_for(&self, rel: &Path) -> Result<&Path, HarnessError> {
        self.workspace_root
            .as_deref()
            .ok_or_else(|| unsupported_fs(rel))
    }
}

impl TerminalBackend for LocalBackend {
    fn read_file(&self, rel: &Path) -> Result<Vec<u8>, HarnessError> {
        local_fs::read_file(self.root_for(rel)?, rel)
    }

    fn write_file(&self, rel: &Path, contents: &[u8]) -> Result<(), HarnessError> {
        local_fs::write_file(self.root_for(rel)?, rel, contents)
    }

    fn delete_file(&self, rel: &Path) -> Result<(), HarnessError> {
        local_fs::delete_file(self.root_for(rel)?, rel)
    }

    fn create_dir_all(&self, rel: &Path) -> Result<(), HarnessError> {
        local_fs::create_dir_all(self.root_for(rel)?, rel)
    }

    fn metadata(&self, rel: &Path) -> Result<FileKind, HarnessError> {
        local_fs::metadata(self.root_for(rel)?, rel)
    }

    fn read_dir(&self, rel: &Path) -> Result<Vec<DirEntry>, HarnessError> {
        local_fs::read_dir(self.root_for(rel)?, rel)
    }

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

    // --- Filesystem op tests (initiative #7, phase 3) -------------------

    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn fs_read_write_delete_round_trip() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let backend = LocalBackend::rooted(root.clone());

        backend
            .write_file(Path::new("a/b.txt"), b"hello")
            .expect("write creates parents");
        assert!(root.join("a/b.txt").is_file());

        let bytes = backend.read_file(Path::new("a/b.txt")).unwrap();
        assert_eq!(bytes, b"hello");

        let meta = backend.metadata(Path::new("a/b.txt")).unwrap();
        assert!(meta.exists && meta.is_file && !meta.is_dir);

        backend.delete_file(Path::new("a/b.txt")).unwrap();
        assert!(!root.join("a/b.txt").exists());

        let meta = backend.metadata(Path::new("a/b.txt")).unwrap();
        assert_eq!(meta, FileKind::MISSING);
    }

    #[test]
    fn fs_create_dir_all_and_read_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let backend = LocalBackend::rooted(root.clone());

        backend.create_dir_all(Path::new("nested/inner")).unwrap();
        assert!(root.join("nested/inner").is_dir());
        backend
            .write_file(Path::new("nested/file.txt"), b"x")
            .unwrap();

        let mut entries = backend.read_dir(Path::new("nested")).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0],
            DirEntry {
                name: "file.txt".into(),
                is_dir: false
            }
        );
        assert_eq!(
            entries[1],
            DirEntry {
                name: "inner".into(),
                is_dir: true
            }
        );

        let meta = backend.metadata(Path::new("nested")).unwrap();
        assert!(meta.is_dir && !meta.is_file);
    }

    #[test]
    fn fs_rejects_parent_dir_traversal() {
        let dir = tempdir().unwrap();
        let backend = LocalBackend::rooted(dir.path().canonicalize().unwrap());

        assert!(matches!(
            backend.read_file(Path::new("../outside.txt")),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        assert!(matches!(
            backend.write_file(Path::new("../outside.txt"), b"x"),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        assert!(matches!(
            backend.create_dir_all(Path::new("../evil")),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
    }

    #[test]
    fn fs_rejects_absolute_paths_for_writes() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let backend = LocalBackend::rooted(dir.path().canonicalize().unwrap());

        let abs = outside.path().join("secret.txt");
        assert!(matches!(
            backend.write_file(&abs, b"x"),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        assert!(matches!(
            backend.create_dir_all(&abs),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
    }

    #[test]
    fn fs_rejects_absolute_path_reads_outside_workspace() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();
        let backend = LocalBackend::rooted(dir.path().canonicalize().unwrap());

        // An absolute path that canonicalizes outside the root is rejected
        // (same as the historical `Workspace::resolve`).
        assert!(matches!(
            backend.read_file(&outside_file),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
    }

    /// The canonicalize-based escape protection: a symlink that lives inside the
    /// workspace but points outside it must be rejected on read AND on delete,
    /// proving symlink-escape protection survived the refactor.
    #[cfg(unix)]
    #[test]
    fn fs_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        // A symlink inside the workspace pointing at a file outside it.
        let link = root.join("escape");
        symlink(&outside_file, &link).unwrap();

        let backend = LocalBackend::rooted(root.clone());

        // Reading through the symlink resolves (canonicalize) to outside the
        // workspace and is rejected.
        assert!(matches!(
            backend.read_file(Path::new("escape")),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        // Deleting through the symlink is likewise rejected.
        assert!(matches!(
            backend.delete_file(Path::new("escape")),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        // The outside file is untouched.
        assert_eq!(fs::read_to_string(&outside_file).unwrap(), "secret");

        // A symlinked *directory* escape: writing a file "through" it must also
        // be rejected (nearest-existing-parent canonicalizes outside the root).
        let outside_dir = tempdir().unwrap();
        let dir_link = root.join("dirlink");
        symlink(outside_dir.path(), &dir_link).unwrap();
        assert!(matches!(
            backend.write_file(Path::new("dirlink/new.txt"), b"x"),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        assert!(!outside_dir.path().join("new.txt").exists());
    }

    #[test]
    fn unrooted_local_backend_fs_ops_error() {
        let backend = LocalBackend::new();
        assert!(backend.read_file(Path::new("x.txt")).is_err());
        assert!(backend.write_file(Path::new("x.txt"), b"x").is_err());
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
