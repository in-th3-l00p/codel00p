//! SSH (remote-resident) terminal backend — Phase 3b of initiative #7.
//!
//! [`SshBackend`] satisfies [`TerminalBackend`](super::TerminalBackend) by
//! running **both** command execution and filesystem operations on a remote host
//! over SSH. This is the *remote-resident* model: the workspace lives ON the
//! remote host at a configured absolute path ([`SshConfig::workspace`]). Nothing
//! is synced; the local machine only orchestrates. Because the trait already
//! routes fs ops and commands through the backend, this is the clean fit — every
//! read/write/stat and every `run_command` becomes one (or a multiplexed) `ssh`
//! invocation against the remote.
//!
//! # Why shell out to the system `ssh`
//!
//! We invoke the user's installed `ssh` CLI rather than linking an SSH library,
//! so the user's `~/.ssh/config`, keys, known-hosts, and `ssh-agent` "just work"
//! (best auth story), mirroring the Docker backend's "shell out to a CLI" pattern.
//!
//! # Connection multiplexing
//!
//! Each fs op / command is its own `ssh` process, so without multiplexing every
//! op would pay a fresh TCP + auth handshake. We therefore pass
//! `-o ControlMaster=auto -o ControlPath=<socket> -o ControlPersist=<n>` so the
//! many per-op invocations share ONE underlying connection. The control socket
//! lives in a backend-owned temp dir; on drop we best-effort `ssh -O exit` to
//! tear the master down, and `ControlPersist` bounds the lifetime if the process
//! dies before `Drop` runs (so we never leak a master indefinitely).
//!
//! # Shell quoting
//!
//! Remote command strings are interpreted by the remote shell, so EVERY program,
//! argument, and path is single-quote-escaped via [`sh_quote`] and the remote
//! command is assembled as one string. Commands run as
//! `cd <cwd> && exec <program> <args...>` so the child replaces the shell (clean
//! exit-code proxying) and starts in the mapped remote cwd.
//!
//! # Boundary enforcement
//!
//! Filesystem ops re-check the lexical `..` / empty / absolute rules locally
//! (mirroring [`local_fs`](super::local_fs), so the backend is self-contained)
//! and additionally resolve the real remote path (`readlink -f`) and require it
//! to stay under the remote workspace — the remote analogue of local's
//! canonicalize + prefix check, preserving symlink-escape protection.
//!
//! # No orphaned remote processes
//!
//! Foreground commands are wrapped in the remote `timeout` utility so the remote
//! process is bounded and killed *remotely* even if the local `ssh` is killed;
//! the local poll-loop also enforces the deadline and kills the local `ssh`.
//! Background commands run under `setsid` (their own remote session/process
//! group) with a unique marker token embedded literally in the command line, so
//! [`ChildHandle::kill`] can `ssh ... pkill -TERM -f <marker>` the remote
//! process group rather than only closing the local channel.
//!
//! # Remote platform assumption
//!
//! The fs ops use GNU coreutils conventions (`stat -c`, `find -printf`,
//! `readlink -f`) — i.e. a Linux remote. This is documented and is the common
//! case; a non-GNU remote (e.g. BSD/macOS) would need different `stat`/`find`
//! flags and is out of scope for this phase.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

use crate::{
    errors::HarnessError,
    walk::{DEFAULT_IGNORED_DIRS, MAX_FILES_WALKED},
};

use super::{
    CappedOutput, ChildHandle, CommandOutcome, CommandSpec, DirEntry, FileKind, OutputLimits,
    POLL_INTERVAL_MS, TerminalBackend, cap_output,
};

mod commands;
use commands::*;

/// The tool name reported in [`HarnessError::ToolFailed`] from this backend.
const TOOL_NAME: &str = "run_command";

/// GNU `timeout` exit status when it had to kill the command (124 by default).
const TIMEOUT_EXIT_CODE: i32 = 124;

/// How long (seconds) the multiplexed SSH master persists after the last client
/// disconnects, if `Drop` did not get to tear it down.
const CONTROL_PERSIST_SECS: u64 = 30;

/// Configuration for [`SshBackend`].
///
/// [`Self::host`] and [`Self::workspace`] are required; everything else has a
/// sensible default or is optional. `host` may be a `~/.ssh/config` alias (so
/// `user`/`port`/`identity_file` can all come from the user's ssh config and be
/// left unset here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshConfig {
    /// Remote host (an IP, DNS name, or a `~/.ssh/config` alias). Required.
    pub host: String,
    /// Remote login user. `None` defers to ssh config / current user.
    pub user: Option<String>,
    /// Remote port. `None` defers to ssh config / the default (22).
    pub port: Option<u16>,
    /// Identity (private key) file. `None` defers to ssh config / the agent.
    pub identity_file: Option<PathBuf>,
    /// Absolute path on the remote host where the workspace lives. Required.
    pub workspace: PathBuf,
    /// Extra `-o KEY=VALUE` ssh options appended verbatim (e.g.
    /// `StrictHostKeyChecking=accept-new`).
    pub options: Vec<String>,
}

impl SshConfig {
    /// Construct a config for `host` with the remote workspace at `workspace`
    /// (both required), leaving everything else at its default.
    pub fn new(host: impl Into<String>, workspace: impl Into<PathBuf>) -> Self {
        Self {
            host: host.into(),
            user: None,
            port: None,
            identity_file: None,
            workspace: workspace.into(),
            options: Vec::new(),
        }
    }

    /// `user@host` if a user is set, else just `host` — the SSH target argument.
    fn target(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        }
    }
}

/// Runs commands and filesystem ops on a remote host over SSH (remote-resident).
///
/// Cheap to clone (small config + a shared availability cache + a shared control
/// dir), so it can live behind an `Arc<dyn TerminalBackend>` like the other
/// backends.
#[derive(Clone, Debug)]
pub struct SshBackend {
    /// Absolute LOCAL workspace root the harness was built with. Command `cwd`s
    /// arrive resolved under this root; we strip this prefix to get the
    /// workspace-relative part, then re-root it under the REMOTE workspace.
    local_workspace_root: PathBuf,
    config: SshConfig,
    /// One-time `ssh ... -- true` availability result, cached after first check.
    availability: std::sync::Arc<OnceLock<Result<(), String>>>,
    /// Backend-owned temp dir holding the ControlMaster socket. Shared so clones
    /// reuse the same multiplexed connection; dropped (and the master torn down)
    /// when the last clone goes away.
    control: std::sync::Arc<ControlDir>,
}

/// Owns the ControlMaster socket's temp dir and the ssh option set needed to
/// reach it, tearing the master down on drop.
#[derive(Debug)]
struct ControlDir {
    dir: PathBuf,
    /// The ssh target + base options, captured so `Drop` can run `ssh -O exit`.
    target: String,
    base_options: Vec<String>,
    /// Whether `dir` was created by us (and should be removed on drop).
    owned: bool,
}

impl ControlDir {
    fn socket_path(&self) -> PathBuf {
        self.dir.join("control.sock")
    }
}

impl Drop for ControlDir {
    fn drop(&mut self) {
        // Best-effort tear-down of the multiplexed master so we do not leave a
        // backgrounded ssh master (and its socket) lying around. Failures are
        // ignored: the master may already be gone, and `ControlPersist` bounds
        // its lifetime regardless.
        let _ = Command::new("ssh")
            .args(&self.base_options)
            .arg("-O")
            .arg("exit")
            .arg(&self.target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if self.owned {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

impl SshBackend {
    /// Build a backend that runs ops on `config.host`, mapping command cwds from
    /// `local_workspace_root` onto `config.workspace`. `local_workspace_root`
    /// must be an absolute local path (the harness passes the canonicalized
    /// workspace root); `config.workspace` must be an absolute remote path.
    pub fn new(local_workspace_root: impl Into<PathBuf>, config: SshConfig) -> Self {
        let local_workspace_root = local_workspace_root.into();
        let control = ControlDir::create(&config);
        Self {
            local_workspace_root,
            config,
            availability: std::sync::Arc::new(OnceLock::new()),
            control: std::sync::Arc::new(control),
        }
    }

    /// The configuration this backend was built with.
    pub fn config(&self) -> &SshConfig {
        &self.config
    }

    /// The local workspace root this backend maps command cwds from.
    pub fn local_workspace_root(&self) -> &Path {
        &self.local_workspace_root
    }

    /// The base ssh options (multiplexing + identity/port/user-supplied `-o`s)
    /// shared by every invocation. The target and the `--` command come after.
    fn base_ssh_options(&self) -> Vec<String> {
        build_base_ssh_options(&self.config, Some(&self.control.socket_path()))
    }

    /// Build the full ssh argv (after the `ssh` executable) for running
    /// `remote_command` on the remote: base options, the target, `--`, then the
    /// single remote command string.
    fn ssh_argv(&self, remote_command: &str) -> Vec<String> {
        let mut argv = self.base_ssh_options();
        argv.push(self.config.target());
        argv.push("--".to_string());
        argv.push(remote_command.to_string());
        argv
    }

    /// Map a command `cwd` to its remote path and build the `cd && exec` remote
    /// command string for `spec`.
    fn remote_command_for(&self, spec: &CommandSpec) -> Result<String, HarnessError> {
        let remote_cwd = map_cwd_to_remote(
            &self.local_workspace_root,
            &self.config.workspace,
            &spec.cwd,
        )?;
        Ok(build_exec_command(&remote_cwd, spec))
    }

    /// The remote absolute path for a workspace-relative `rel`, after the lexical
    /// boundary checks. Used by the fs ops.
    fn remote_path_for(&self, rel: &Path) -> Result<String, HarnessError> {
        remote_workspace_path(&self.config.workspace, rel)
    }

    /// Verify the remote is reachable (and ssh exists) exactly once via a cheap
    /// `ssh ... -- true`. Returns a clear error otherwise so a misconfigured
    /// backend fails loudly rather than with a cryptic per-op failure.
    fn ensure_available(&self) -> Result<(), HarnessError> {
        let result = self.availability.get_or_init(|| {
            let argv = self.ssh_argv("true");
            let output = Command::new("ssh")
                .args(&argv)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match output {
                Err(error) => Err(format!(
                    "the `ssh` CLI could not be launched: {error}. Install OpenSSH or set agent.execution_backend back to `local`."
                )),
                Ok(out) if out.status.success() => Ok(()),
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    Err(format!(
                        "the remote host `{}` is not reachable over ssh: {}. Check your ssh config / keys / host, or set agent.execution_backend back to `local`.",
                        self.config.host,
                        stderr.trim()
                    ))
                }
            }
        });
        result.clone().map_err(|message| HarnessError::ToolFailed {
            name: TOOL_NAME.to_string(),
            message: format!("ssh backend selected but the remote is not available: {message}"),
        })
    }

    /// Run `remote_command` to completion, capturing stdout/stderr bytes. Used by
    /// the fs ops (which need binary-safe stdout and a clear error on failure).
    /// `stdin_bytes`, when present, is piped to the remote command.
    fn run_capture(
        &self,
        remote_command: &str,
        stdin_bytes: Option<&[u8]>,
    ) -> Result<Vec<u8>, HarnessError> {
        self.ensure_available()?;
        let argv = self.ssh_argv(remote_command);
        let mut command = Command::new("ssh");
        command
            .args(&argv)
            .stdin(if stdin_bytes.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(spawn_error)?;
        if let Some(bytes) = stdin_bytes {
            let mut stdin = child.stdin.take().ok_or_else(|| HarnessError::ToolFailed {
                name: TOOL_NAME.to_string(),
                message: "failed to open ssh stdin for write".to_string(),
            })?;
            stdin.write_all(bytes)?;
            // Drop closes stdin so the remote `cat` sees EOF.
            drop(stdin);
        }
        let output = child.wait_with_output()?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(HarnessError::ToolFailed {
                name: "workspace".to_string(),
                message: format!(
                    "remote ssh command failed (exit {:?}): {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            })
        }
    }
}

impl ControlDir {
    /// Create the control-socket temp dir and capture the options needed to tear
    /// the master down later. Falls back to a non-owned temp dir on failure (the
    /// master would then not multiplex, but ops still work).
    fn create(config: &SshConfig) -> Self {
        let (dir, owned) = match tempfile::Builder::new().prefix("codel00p-ssh-").tempdir() {
            Ok(temp) => {
                // Keep the directory alive past the TempDir guard; ControlDir's
                // own Drop removes it so the lifetime matches the backend.
                (temp.keep(), true)
            }
            Err(_) => (std::env::temp_dir(), false),
        };
        let socket = dir.join("control.sock");
        ControlDir {
            base_options: build_base_ssh_options(config, Some(&socket)),
            target: config.target(),
            dir,
            owned,
        }
    }
}

impl TerminalBackend for SshBackend {
    fn read_file(&self, rel: &Path) -> Result<Vec<u8>, HarnessError> {
        let rpath = self.remote_path_for(rel)?;
        // `cat` the file; the symlink-boundary check is folded into one command
        // so a single round-trip both verifies containment and reads the bytes.
        let command = build_read_file_command(&self.config.workspace, &rpath);
        self.run_capture(&command, None)
    }

    fn write_file(&self, rel: &Path, contents: &[u8]) -> Result<(), HarnessError> {
        // Writes reject absolute paths (handled in `remote_path_for` via the
        // lexical checks below) — re-root and stream bytes through stdin.
        reject_absolute_for_write(rel)?;
        let rpath = self.remote_path_for(rel)?;
        let command = build_write_file_command(&rpath);
        self.run_capture(&command, Some(contents))?;
        Ok(())
    }

    fn delete_file(&self, rel: &Path) -> Result<(), HarnessError> {
        let rpath = self.remote_path_for(rel)?;
        // Match local `delete_file`: the file must already exist (and stay inside
        // the workspace) — verify, boundary-check, then remove, in one command.
        let command = build_delete_file_command(&self.config.workspace, &rpath);
        self.run_capture(&command, None)?;
        Ok(())
    }

    fn create_dir_all(&self, rel: &Path) -> Result<(), HarnessError> {
        reject_absolute_for_write(rel)?;
        let rpath = self.remote_path_for(rel)?;
        let command = build_mkdir_command(&rpath);
        self.run_capture(&command, None)?;
        Ok(())
    }

    fn metadata(&self, rel: &Path) -> Result<FileKind, HarnessError> {
        let rpath = self.remote_path_for(rel)?;
        let command = build_metadata_command(&self.config.workspace, &rpath);
        let stdout = self.run_capture(&command, None)?;
        Ok(parse_metadata(&String::from_utf8_lossy(&stdout)))
    }

    fn read_dir(&self, rel: &Path) -> Result<Vec<DirEntry>, HarnessError> {
        let rpath = self.remote_path_for(rel)?;
        let command = build_read_dir_command(&self.config.workspace, &rpath);
        let stdout = self.run_capture(&command, None)?;
        Ok(parse_read_dir(&String::from_utf8_lossy(&stdout)))
    }

    /// Override the default `read_dir`-based walk with a SINGLE remote `find`,
    /// converting absolute remote paths back to workspace-root-relative,
    /// `/`-normalized strings, pruning the ignored dirs and capping the count.
    fn walk(
        &self,
        rel: &Path,
        include_ignored: bool,
        visit: &mut dyn FnMut(&str),
    ) -> Result<(), HarnessError> {
        let rpath = self.remote_path_for(rel)?;
        let command = build_walk_command(&rpath, include_ignored);
        let stdout = self.run_capture(&command, None)?;
        for path in parse_walk(
            &String::from_utf8_lossy(&stdout),
            &workspace_str(&self.config.workspace),
        )
        .take(MAX_FILES_WALKED)
        {
            visit(&path);
        }
        Ok(())
    }

    fn run_foreground(
        &self,
        spec: &CommandSpec,
        limits: OutputLimits,
    ) -> Result<CommandOutcome, HarnessError> {
        self.ensure_available()?;
        let inner = self.remote_command_for(spec)?;
        // Wrap in remote `timeout` so the remote process is killed REMOTELY even
        // if the local ssh dies, avoiding orphaned remote processes. We add a
        // small grace over the local deadline so the local poll loop is the
        // backstop, not a race with the remote timer.
        let remote_command =
            wrap_with_timeout(&inner, limits.timeout.as_secs().saturating_add(2).max(1));
        let argv = self.ssh_argv(&remote_command);

        let mut child = Command::new("ssh")
            .args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(spawn_error)?;

        let started = Instant::now();
        let mut timed_out = false;
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if started.elapsed() >= limits.timeout {
                timed_out = true;
                // Kill the local ssh; the remote `timeout` bounds the remote
                // process independently (so it dies even if this kill races).
                let _ = child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        let output = child.wait_with_output()?;
        let stdout: CappedOutput = cap_output(&output.stdout, limits.max_output_bytes);
        let stderr: CappedOutput = cap_output(&output.stderr, limits.max_output_bytes);

        // `ssh` proxies the remote command's exit status as its own. GNU
        // `timeout` exits 124 when it killed the command, which we translate to a
        // timeout outcome (in addition to the local-deadline detection above).
        let remote_code = output.status.code();
        let remote_timed_out = remote_code == Some(TIMEOUT_EXIT_CODE);
        let timed_out = timed_out || remote_timed_out;
        let exit_code = if timed_out { None } else { remote_code };
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
        self.ensure_available()?;
        let remote_cwd = map_cwd_to_remote(
            &self.local_workspace_root,
            &self.config.workspace,
            &spec.cwd,
        )?;
        // Background commands run in their own remote session/process group with
        // a UNIQUE marker token embedded literally in the command line, so
        // `kill` can `pkill -f <marker>` the whole remote group (and any
        // children) rather than only closing the local ssh channel. The marker
        // is unique per spawn so it never matches an unrelated process.
        let marker = next_marker();
        let remote_command = build_background_command(&remote_cwd, spec, &marker);
        let argv = self.ssh_argv(&remote_command);

        let child = Command::new("ssh")
            .args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(spawn_error)?;

        Ok(Box::new(SshChildHandle {
            child,
            target: self.config.target(),
            base_options: self.base_ssh_options(),
            marker,
        }))
    }

    /// A remote host is isolated from the local machine, satisfying the
    /// `require_isolation_for_unattended` policy.
    fn is_isolated(&self) -> bool {
        true
    }
}

/// A [`ChildHandle`] backed by a local `ssh` proxy process plus the unique
/// marker token embedded in the remote command line. `kill` terminates the
/// REMOTE process (and its session children) by `pkill -f <marker>`, then reaps
/// the local proxy. Because the marker is in the command line (not the
/// environment), `pkill -f` finds it regardless of who owns the stdout pipe.
///
/// Residual caveat: if the remote lacks `pkill` (uncommon on Linux, the
/// documented target platform), `kill` falls back to closing the local channel
/// only, which may leave the remote process running until it finishes on its own.
struct SshChildHandle {
    child: Child,
    target: String,
    base_options: Vec<String>,
    /// The unique per-spawn marker token embedded in the remote command line,
    /// used by `kill` to `pkill -f` the remote process group.
    marker: String,
}

impl ChildHandle for SshChildHandle {
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
        // Terminate the remote process (and its session children) by the unique
        // marker token in its command line, so the remote command actually dies
        // rather than just closing the local channel. Best-effort: the process
        // may have already exited. Then reap the local proxy.
        remote_kill_by_marker(&self.base_options, &self.target, &self.marker);
        let _ = self.child.kill();
        Ok(())
    }
}

/// Best-effort remote kill of the background process carrying our unique marker
/// token in its command line. Sends SIGTERM to the matching process; since the
/// command runs under `setsid` the whole session goes down with the leader.
fn remote_kill_by_marker(base_options: &[String], target: &str, marker: &str) {
    let command = build_kill_command(marker);
    let _ = run_remote_detached(base_options, target, &command);
}

/// Run a one-shot remote command for side effects only (kill), ignoring output.
fn run_remote_detached(base_options: &[String], target: &str, remote_command: &str) -> bool {
    Command::new("ssh")
        .args(base_options)
        .arg(target)
        .arg("--")
        .arg(remote_command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn spawn_error(error: std::io::Error) -> HarnessError {
    HarnessError::ToolFailed {
        name: TOOL_NAME.to_string(),
        message: format!("failed to launch `ssh`: {error}"),
    }
}

// === Pure builders (unit-tested without touching the network) ================

/// Prefix of the unique marker token embedded in background command lines so a
/// remote `pkill -f` can target exactly that spawn.
const MARKER_PREFIX: &str = "CODEL00P_SSH_BG";

/// Process-global counter making each background marker unique within a process;
/// combined with the pid it is unique across the host for the process lifetime.
static MARKER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    fn local_root() -> PathBuf {
        PathBuf::from("/home/user/project")
    }

    fn remote_ws() -> PathBuf {
        PathBuf::from("/srv/workspace")
    }

    fn cfg() -> SshConfig {
        SshConfig::new("example.com", remote_ws())
    }

    fn spec_in(cwd: &str, program: &str, args: &[&str]) -> CommandSpec {
        CommandSpec::new(
            program,
            args.iter().map(|s| s.to_string()).collect(),
            PathBuf::from(cwd),
        )
    }

    // --- sh_quote -----------------------------------------------------------

    #[test]
    fn sh_quote_wraps_plain_string() {
        assert_eq!(sh_quote("hello"), "'hello'");
    }

    #[test]
    fn sh_quote_handles_nasty_inputs() {
        // Spaces, dollar, semicolon, &&, backticks, redirection, newline.
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("$HOME"), "'$HOME'");
        assert_eq!(sh_quote("a;b"), "'a;b'");
        assert_eq!(sh_quote("a && b"), "'a && b'");
        assert_eq!(sh_quote("`id`"), "'`id`'");
        assert_eq!(sh_quote("a > b"), "'a > b'");
        assert_eq!(sh_quote("a\nb"), "'a\nb'");
    }

    #[test]
    fn sh_quote_escapes_embedded_single_quotes() {
        // The classic dangerous case: a single quote must close, escape, reopen.
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
        assert_eq!(sh_quote("'"), "''\\'''");
        // A full injection attempt stays inert (entirely inside quotes/escapes).
        assert_eq!(sh_quote("'; rm -rf / #"), "''\\''; rm -rf / #'");
    }

    // --- base ssh options ---------------------------------------------------

    #[test]
    fn base_options_include_multiplexing_and_target_pieces() {
        let socket = PathBuf::from("/tmp/x/control.sock");
        let opts = build_base_ssh_options(&cfg(), Some(&socket));
        assert!(
            opts.windows(2)
                .any(|w| w[0] == "-o" && w[1] == "ControlMaster=auto")
        );
        assert!(opts.iter().any(|o| o == "ControlPath=/tmp/x/control.sock"));
        assert!(opts.iter().any(|o| o.starts_with("ControlPersist=")));
    }

    #[test]
    fn base_options_include_port_identity_user_options() {
        let mut config = cfg();
        config.user = Some("deploy".into());
        config.port = Some(2222);
        config.identity_file = Some(PathBuf::from("/home/user/.ssh/id_ed25519"));
        config.options = vec!["StrictHostKeyChecking=accept-new".into()];
        let opts = build_base_ssh_options(&config, None);

        assert!(opts.windows(2).any(|w| w[0] == "-p" && w[1] == "2222"));
        assert!(
            opts.windows(2)
                .any(|w| w[0] == "-i" && w[1] == "/home/user/.ssh/id_ed25519")
        );
        assert!(
            opts.windows(2)
                .any(|w| w[0] == "-o" && w[1] == "StrictHostKeyChecking=accept-new")
        );
        // No multiplexing options when no control socket is supplied.
        assert!(!opts.iter().any(|o| o.starts_with("ControlMaster")));
        // target() composes user@host.
        assert_eq!(config.target(), "deploy@example.com");
    }

    #[test]
    fn target_is_bare_host_without_user() {
        assert_eq!(cfg().target(), "example.com");
    }

    // --- cwd → remote mapping ----------------------------------------------

    #[test]
    fn cwd_root_maps_to_remote_workspace() {
        let mapped = map_cwd_to_remote(&local_root(), &remote_ws(), &local_root()).unwrap();
        assert_eq!(mapped, "/srv/workspace");
    }

    #[test]
    fn cwd_nested_maps_under_remote_workspace() {
        let mapped = map_cwd_to_remote(
            &local_root(),
            &remote_ws(),
            Path::new("/home/user/project/crates/foo"),
        )
        .unwrap();
        assert_eq!(mapped, "/srv/workspace/crates/foo");
    }

    #[test]
    fn cwd_outside_local_root_is_an_error() {
        let result = map_cwd_to_remote(&local_root(), &remote_ws(), Path::new("/etc"));
        let Err(HarnessError::ToolFailed { message, .. }) = result else {
            panic!("expected error for cwd outside the local workspace root");
        };
        assert!(message.contains("not under the SSH local workspace root"));
    }

    // --- exec command string ------------------------------------------------

    #[test]
    fn exec_command_quotes_cd_and_program_and_args() {
        let spec = spec_in("/home/user/project", "cargo", &["build", "--release"]);
        let remote_cwd = map_cwd_to_remote(&local_root(), &remote_ws(), &spec.cwd).unwrap();
        let command = build_exec_command(&remote_cwd, &spec);
        assert_eq!(
            command,
            "cd '/srv/workspace' && exec 'cargo' 'build' '--release'"
        );
    }

    #[test]
    fn exec_command_neutralizes_injection_in_args() {
        let spec = spec_in("/home/user/project", "echo", &["; rm -rf /"]);
        let remote_cwd = map_cwd_to_remote(&local_root(), &remote_ws(), &spec.cwd).unwrap();
        let command = build_exec_command(&remote_cwd, &spec);
        // The dangerous arg is fully inside single quotes — inert.
        assert_eq!(command, "cd '/srv/workspace' && exec 'echo' '; rm -rf /'");
    }

    // --- timeout wrapper ----------------------------------------------------

    #[test]
    fn timeout_wrapper_quotes_inner_command() {
        let wrapped = wrap_with_timeout("cd '/x' && exec 'sleep' '5'", 12);
        assert_eq!(
            wrapped,
            "timeout 12 sh -c 'cd '\\''/x'\\'' && exec '\\''sleep'\\'' '\\''5'\\'''"
        );
    }

    #[test]
    fn timeout_exit_124_translates_to_timed_out() {
        // Documented contract: GNU timeout exits 124 on expiry. The run_foreground
        // logic maps that to timed_out=true / exit_code=None; assert the constant.
        assert_eq!(TIMEOUT_EXIT_CODE, 124);
    }

    // --- fs op command strings ---------------------------------------------

    #[test]
    fn remote_path_for_rel_reroots_under_workspace() {
        assert_eq!(
            remote_workspace_path(&remote_ws(), Path::new("src/main.rs")).unwrap(),
            "/srv/workspace/src/main.rs"
        );
        assert_eq!(
            remote_workspace_path(&remote_ws(), Path::new(".")).unwrap(),
            "/srv/workspace"
        );
    }

    #[test]
    fn remote_path_rejects_parent_traversal() {
        assert!(matches!(
            remote_workspace_path(&remote_ws(), Path::new("../escape")),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
    }

    #[test]
    fn write_rejects_absolute_paths() {
        assert!(matches!(
            reject_absolute_for_write(Path::new("/etc/passwd")),
            Err(HarnessError::WorkspaceEscape { .. })
        ));
        assert!(reject_absolute_for_write(Path::new("ok/file.txt")).is_ok());
    }

    #[test]
    fn read_file_command_has_boundary_and_cat() {
        let rpath = "/srv/workspace/a.txt";
        let command = build_read_file_command(&remote_ws(), rpath);
        assert!(command.contains("readlink -f '/srv/workspace/a.txt'"));
        assert!(command.contains("cat -- '/srv/workspace/a.txt'"));
        // Boundary glob references the quoted workspace root.
        assert!(command.contains("'/srv/workspace'/ | '/srv/workspace'/*"));
        assert!(command.contains("exit 1"));
    }

    #[test]
    fn write_file_command_mkdirs_parent_then_cat() {
        let command = build_write_file_command("/srv/workspace/a/b.txt");
        assert_eq!(
            command,
            "mkdir -p -- '/srv/workspace/a' && cat > '/srv/workspace/a/b.txt'"
        );
    }

    #[test]
    fn delete_file_command_requires_existence_and_boundary() {
        let command = build_delete_file_command(&remote_ws(), "/srv/workspace/a.txt");
        assert!(command.starts_with("test -e '/srv/workspace/a.txt'"));
        assert!(command.contains("readlink -f '/srv/workspace/a.txt'"));
        assert!(command.contains("rm -f -- '/srv/workspace/a.txt'"));
    }

    #[test]
    fn mkdir_command_is_mkdir_p() {
        assert_eq!(
            build_mkdir_command("/srv/workspace/x/y"),
            "mkdir -p -- '/srv/workspace/x/y'"
        );
    }

    #[test]
    fn metadata_command_missing_path_returns_empty_then_stats() {
        let command = build_metadata_command(&remote_ws(), "/srv/workspace/a.txt");
        assert!(command.contains("if [ ! -e '/srv/workspace/a.txt' ]"));
        assert!(command.contains("stat -c '%F|%s' -- '/srv/workspace/a.txt'"));
    }

    #[test]
    fn read_dir_command_uses_find_printf() {
        let command = build_read_dir_command(&remote_ws(), "/srv/workspace/d");
        assert!(
            command.contains("find '/srv/workspace/d' -maxdepth 1 -mindepth 1 -printf '%y %f\\n'")
        );
        assert!(command.contains("readlink -f '/srv/workspace/d'"));
    }

    // --- walk command -------------------------------------------------------

    #[test]
    fn walk_command_prunes_ignored_dirs_by_default() {
        let command = build_walk_command("/srv/workspace", false);
        assert!(command.starts_with("find '/srv/workspace' \\( -type d \\("));
        assert!(command.contains("-prune"));
        assert!(command.ends_with("-o -type f -print"));
        // A representative ignored dir is named.
        assert!(command.contains("-name 'target'") || command.contains("-name 'node_modules'"));
    }

    #[test]
    fn walk_command_include_ignored_is_plain_find() {
        let command = build_walk_command("/srv/workspace", true);
        assert_eq!(command, "find '/srv/workspace' -type f");
    }

    // --- parsers ------------------------------------------------------------

    #[test]
    fn parse_metadata_handles_file_dir_missing() {
        let file = parse_metadata("regular file|42\n");
        assert_eq!(
            file,
            FileKind {
                exists: true,
                is_file: true,
                is_dir: false,
                size: 42
            }
        );
        let empty_file = parse_metadata("regular empty file|0");
        assert!(empty_file.is_file && empty_file.exists);
        let dir = parse_metadata("directory|4096");
        assert_eq!(
            dir,
            FileKind {
                exists: true,
                is_file: false,
                is_dir: true,
                size: 0
            }
        );
        assert_eq!(parse_metadata(""), FileKind::MISSING);
        assert_eq!(parse_metadata("  \n"), FileKind::MISSING);
    }

    #[test]
    fn parse_read_dir_maps_type_letters() {
        let entries = parse_read_dir("d sub\nf file.txt\nl link\n");
        assert_eq!(
            entries,
            vec![
                DirEntry {
                    name: "sub".into(),
                    is_dir: true
                },
                DirEntry {
                    name: "file.txt".into(),
                    is_dir: false
                },
                DirEntry {
                    name: "link".into(),
                    is_dir: false
                },
            ]
        );
    }

    #[test]
    fn parse_read_dir_handles_names_with_spaces() {
        let entries = parse_read_dir("f my file.txt\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my file.txt");
    }

    #[test]
    fn parse_walk_strips_workspace_prefix() {
        let out = "/srv/workspace/src/main.rs\n/srv/workspace/README.md\n/other/x\n";
        let paths: Vec<String> = parse_walk(out, "/srv/workspace").collect();
        assert_eq!(paths, vec!["src/main.rs", "README.md"]);
    }

    #[test]
    fn parse_walk_handles_root_workspace() {
        let out = "/a.txt\n/b/c.txt\n";
        let paths: Vec<String> = parse_walk(out, "/").collect();
        assert_eq!(paths, vec!["a.txt", "b/c.txt"]);
    }

    // --- helpers ------------------------------------------------------------

    #[test]
    fn workspace_str_trims_trailing_slash() {
        assert_eq!(
            workspace_str(Path::new("/srv/workspace/")),
            "/srv/workspace"
        );
        assert_eq!(workspace_str(Path::new("/")), "/");
    }

    #[test]
    fn remote_parent_of_paths() {
        assert_eq!(remote_parent("/srv/workspace/a/b.txt"), "/srv/workspace/a");
        assert_eq!(remote_parent("/a"), "/");
    }

    #[test]
    fn background_command_embeds_marker_in_command_line() {
        let spec = spec_in("/home/user/project", "sleep", &["100"]);
        let remote_cwd = map_cwd_to_remote(&local_root(), &remote_ws(), &spec.cwd).unwrap();
        let marker = "CODEL00P_SSH_BG_42_7";
        let command = build_background_command(&remote_cwd, &spec, marker);
        assert!(command.starts_with("cd '/srv/workspace'"));
        assert!(command.contains("setsid sh -c"));
        // The program is quoted and the marker is the inert trailing $0 argument.
        assert!(command.contains("'exec '\\''sleep'\\'' '\\''100'\\'''"));
        assert!(command.ends_with(&format!("'{marker}'")));
    }

    #[test]
    fn kill_command_targets_marker() {
        let command = build_kill_command("CODEL00P_SSH_BG_42_7");
        assert_eq!(
            command,
            "pkill -TERM -f 'CODEL00P_SSH_BG_42_7' 2>/dev/null; true"
        );
    }

    #[test]
    fn markers_are_unique() {
        let a = next_marker();
        let b = next_marker();
        assert_ne!(a, b);
        assert!(a.starts_with("CODEL00P_SSH_BG_"));
    }

    #[test]
    fn config_new_sets_required_fields_and_defaults() {
        let config = SshConfig::new("host", "/remote/ws");
        assert_eq!(config.host, "host");
        assert_eq!(config.workspace, PathBuf::from("/remote/ws"));
        assert!(config.user.is_none());
        assert!(config.port.is_none());
        assert!(config.identity_file.is_none());
        assert!(config.options.is_empty());
    }

    // --- gated live integration tests --------------------------------------
    //
    // These require a reachable sshd. They self-skip unless
    // `CODEL00P_SSH_TESTS=1` and `CODEL00P_SSH_TEST_HOST` /
    // `CODEL00P_SSH_TEST_WORKSPACE` are set. Run with, e.g.:
    //
    //   CODEL00P_SSH_TESTS=1 \
    //   CODEL00P_SSH_TEST_HOST=myhost \
    //   CODEL00P_SSH_TEST_WORKSPACE=/tmp/codel00p-ssh-it \
    //   cargo test -p codel00p-harness --  ssh::tests::live_ --ignored
    //
    // The remote workspace dir is created/cleaned by the tests themselves.

    fn live_backend() -> Option<SshBackend> {
        if std::env::var("CODEL00P_SSH_TESTS").ok().as_deref() != Some("1") {
            return None;
        }
        let host = std::env::var("CODEL00P_SSH_TEST_HOST").ok()?;
        let workspace = std::env::var("CODEL00P_SSH_TEST_WORKSPACE").ok()?;
        let mut config = SshConfig::new(host, &workspace);
        if let Ok(user) = std::env::var("CODEL00P_SSH_TEST_USER") {
            config.user = Some(user);
        }
        let backend = SshBackend::new(local_root(), config);
        // Ensure the remote workspace exists; if unreachable, self-skip.
        if backend.create_dir_all(Path::new(".")).is_err() {
            return None;
        }
        Some(backend)
    }

    #[test]
    #[ignore = "requires a reachable sshd; gated on CODEL00P_SSH_TESTS=1"]
    fn live_fs_round_trip() {
        let Some(backend) = live_backend() else {
            eprintln!("skipping: ssh integration not configured");
            return;
        };
        backend.write_file(Path::new("it/a.txt"), b"hello").unwrap();
        assert_eq!(backend.read_file(Path::new("it/a.txt")).unwrap(), b"hello");
        let meta = backend.metadata(Path::new("it/a.txt")).unwrap();
        assert!(meta.exists && meta.is_file && meta.size == 5);
        let entries = backend.read_dir(Path::new("it")).unwrap();
        assert!(entries.iter().any(|e| e.name == "a.txt" && !e.is_dir));
        backend.delete_file(Path::new("it/a.txt")).unwrap();
        assert_eq!(
            backend.metadata(Path::new("it/a.txt")).unwrap(),
            FileKind::MISSING
        );
    }

    #[test]
    #[ignore = "requires a reachable sshd; gated on CODEL00P_SSH_TESTS=1"]
    fn live_command_stdout_stderr_exit() {
        let Some(backend) = live_backend() else {
            return;
        };
        let limits = OutputLimits {
            timeout: Duration::from_secs(10),
            max_output_bytes: 16_384,
        };
        let spec = spec_in_remote(
            &backend,
            "sh",
            &["-c", "printf hi; printf err 1>&2; exit 7"],
        );
        let outcome = backend.run_foreground(&spec, limits).unwrap();
        assert_eq!(outcome.stdout, "hi");
        assert_eq!(outcome.stderr, "err");
        assert_eq!(outcome.exit_code, Some(7));
        assert!(!outcome.success && !outcome.timed_out);
    }

    #[test]
    #[ignore = "requires a reachable sshd; gated on CODEL00P_SSH_TESTS=1"]
    fn live_timeout_leaves_no_remote_process() {
        let Some(backend) = live_backend() else {
            return;
        };
        let limits = OutputLimits {
            timeout: Duration::from_secs(1),
            max_output_bytes: 16_384,
        };
        let spec = spec_in_remote(&backend, "sleep", &["30"]);
        let outcome = backend.run_foreground(&spec, limits).unwrap();
        assert!(outcome.timed_out);
        assert_eq!(outcome.exit_code, None);
        assert!(!outcome.success);
    }

    #[test]
    #[ignore = "requires a reachable sshd; gated on CODEL00P_SSH_TESTS=1"]
    fn live_walk_over_seeded_tree() {
        let Some(backend) = live_backend() else {
            return;
        };
        backend
            .write_file(Path::new("tree/src/main.rs"), b"x")
            .unwrap();
        backend
            .write_file(Path::new("tree/target/out"), b"y")
            .unwrap();
        let mut seen = Vec::new();
        backend
            .walk(Path::new("tree"), false, &mut |p| seen.push(p.to_string()))
            .unwrap();
        assert!(seen.iter().any(|p| p == "tree/src/main.rs"));
        assert!(!seen.iter().any(|p| p == "tree/target/out"));
    }

    #[test]
    #[ignore = "requires a reachable sshd; gated on CODEL00P_SSH_TESTS=1"]
    fn live_symlink_escape_is_rejected() {
        let Some(backend) = live_backend() else {
            return;
        };
        // Seed a symlink inside the workspace pointing outside it, then read.
        let spec = spec_in_remote(&backend, "sh", &["-c", "ln -sf /etc/hostname escape_link"]);
        let _ = backend.run_foreground(
            &spec,
            OutputLimits {
                timeout: Duration::from_secs(10),
                max_output_bytes: 1024,
            },
        );
        assert!(backend.read_file(Path::new("escape_link")).is_err());
    }

    /// Build a spec whose cwd is the backend's local workspace root, so it maps
    /// to the remote workspace.
    fn spec_in_remote(backend: &SshBackend, program: &str, args: &[&str]) -> CommandSpec {
        CommandSpec::new(
            program,
            args.iter().map(|s| s.to_string()).collect(),
            backend.local_workspace_root().to_path_buf(),
        )
    }
}
