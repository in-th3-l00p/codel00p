//! Docker terminal backend — Phase 2 of initiative #7, with the warm
//! (long-lived) container optimization.
//!
//! [`DockerBackend`] satisfies [`TerminalBackend`](super::TerminalBackend) by
//! running commands in a container with the agent's workspace bind-mounted. It
//! mirrors [`LocalBackend`](super::LocalBackend) exactly where it can — same
//! `Stdio` wiring, same timeout poll loop, same output capping, same
//! exit-code/`timed_out` semantics — and adds the container lifecycle
//! guarantees on top.
//!
//! # Two execution mechanisms, identical observable semantics
//!
//! The backend supports two mechanisms, selected by
//! [`DockerConfig::reuse_container`]:
//!
//! * **Warm / long-lived container (default, the perf win).** One container per
//!   backend instance is started lazily on the first command via `docker run -d`
//!   with a keepalive (`tail -f /dev/null`); every subsequent command runs in it
//!   via `docker exec`, amortizing container startup across the whole session.
//!   Resource limits / network / user / the bind mount are applied once at
//!   **create** time. The container is removed on [`Drop`].
//! * **Ephemeral per-command container.** The original behavior: each command is
//!   a fresh `docker run --rm`. Selected by `reuse_container = false`, kept as an
//!   escape hatch (and so the old path stays tested).
//!
//! Both produce identical [`CommandOutcome`]s (same stdout/stderr, capping,
//! exit-code/`timed_out`, and cwd); only the *mechanism* differs.
//!
//! # Lifecycle / no orphaned containers (warm path)
//!
//! * **Lazy start.** The container is created on the first foreground/background
//!   command and cached behind interior mutability (the trait methods take
//!   `&self`).
//! * **Teardown on Drop.** [`WarmContainer`]'s `Drop` runs `docker rm -f <name>`
//!   for the started container (best-effort), so when the backend (which lives
//!   for the session/turn) is dropped, the container is removed.
//! * **Crash-safe stale reaper.** `Drop` does not run on `SIGKILL`. To bound
//!   leaks we label every warm container with `codel00p.kind=warm` and
//!   `codel00p.pid=<pid>`, and at lazy start we best-effort reap any warm
//!   container whose recorded pid belongs to a **dead** process (conservative:
//!   only containers from a no-longer-running pid are removed).
//!
//! # Timeout / kill (warm path)
//!
//! Because `docker exec` does not stop the in-container process when the local
//! `docker exec` proxy is killed, we bound the in-container process directly:
//!
//! * **Foreground timeout.** The exec'd command is wrapped with the in-container
//!   `timeout <secs> <program> <args>` (mirroring the SSH backend), so it
//!   self-bounds remotely even if the local proxy dies. `timeout` exits 124 when
//!   it fires, which is translated to `timed_out`. The local poll loop is the
//!   backstop.
//! * **Background kill.** The exec'd command embeds a unique marker token in its
//!   command line; [`ChildHandle::kill`] runs `docker exec <name> pkill -TERM -f
//!   <marker>` to terminate that one in-container process group, then reaps the
//!   local proxy. The shared container is left alive (only this command dies),
//!   contrasting with the ephemeral path which `docker kill`s the whole
//!   container.
//!
//! # Keepalive choice
//!
//! The warm container's entrypoint is `tail -f /dev/null`, which blocks forever
//! on every common base image (including `alpine`, whose BusyBox `sleep` lacks
//! `sleep infinity`). It uses no CPU and is trivially `docker rm -f`-able.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::errors::HarnessError;

use super::{
    CappedOutput, ChildHandle, CommandOutcome, CommandSpec, DirEntry, FileKind, OutputLimits,
    POLL_INTERVAL_MS, TerminalBackend, cap_output, local_fs,
};

/// The tool name reported in [`HarnessError::ToolFailed`] from this backend.
const TOOL_NAME: &str = "run_command";

/// GNU/BusyBox `timeout` exit status when it had to kill the command (124).
const TIMEOUT_EXIT_CODE: i32 = 124;

/// Keepalive entrypoint for the warm container. `tail -f /dev/null` blocks
/// forever on every common base image (including alpine/BusyBox, whose `sleep`
/// lacks `sleep infinity`) at near-zero cost.
const KEEPALIVE: &[&str] = &["tail", "-f", "/dev/null"];

/// Label marking a container as a codel00p warm container (for the reaper).
const LABEL_KIND: &str = "codel00p.kind=warm";
/// Label key carrying the owning process's pid (for the stale reaper).
const LABEL_PID_KEY: &str = "codel00p.pid";

/// Configuration for [`DockerBackend`]. Sensible defaults are provided by
/// [`Default`]; the one field with no universal default is [`Self::image`],
/// which defaults to a small, ubiquitous base image (`alpine`) so the backend
/// is usable out of the box. Override it for project-specific toolchains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerConfig {
    /// Image to run commands in (e.g. `alpine`, `rust:1`, a project image).
    pub image: String,
    /// Absolute path inside the container where the workspace root is mounted.
    pub container_mount: PathBuf,
    /// `--memory` limit (e.g. `512m`, `2g`). `None` leaves it unset.
    pub memory: Option<String>,
    /// `--cpus` limit (e.g. `1.5`). `None` leaves it unset.
    pub cpus: Option<String>,
    /// `--network` mode (e.g. `none`, `bridge`, `host`). Defaults to `none`:
    /// commands cannot reach the network unless explicitly opted in, matching
    /// the "sandbox" intent of an isolating backend.
    pub network: Option<String>,
    /// Run the container as the host process's real uid:gid so bind-mounted
    /// files stay host-owned (Unix only; ignored on other platforms).
    pub map_host_user: bool,
    /// Extra `-e KEY=VALUE` environment entries passed into the container.
    pub env: Vec<(String, String)>,
    /// Reuse ONE long-lived container per backend instance (started lazily,
    /// commands run via `docker exec`) instead of a fresh `docker run --rm` per
    /// command. Default `true` — the perf win. Set `false` for the ephemeral
    /// per-command behavior (the escape hatch).
    pub reuse_container: bool,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            image: "alpine".to_string(),
            container_mount: PathBuf::from("/workspace"),
            memory: None,
            cpus: None,
            network: Some("none".to_string()),
            map_host_user: true,
            env: Vec::new(),
            reuse_container: true,
        }
    }
}

impl DockerConfig {
    /// Construct a config for `image`, otherwise default.
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            ..Self::default()
        }
    }
}

/// Runs commands inside Docker containers with the workspace mounted.
///
/// Cheap to clone (the config is small and the shared state lives behind an
/// `Arc`), so it can live behind an `Arc<dyn TerminalBackend>` like
/// [`LocalBackend`]. Clones share the SAME warm container; the container is torn
/// down when the last clone is dropped.
#[derive(Clone, Debug)]
pub struct DockerBackend {
    /// Absolute host path of the workspace root (mounted into the container).
    workspace_root: PathBuf,
    config: DockerConfig,
    /// One-time `docker version` availability result, cached after first check.
    availability: std::sync::Arc<OnceLock<Result<(), String>>>,
    /// The lazily-started warm container, shared across clones and torn down on
    /// last-drop. `None` (and unused) when `reuse_container` is false.
    warm: std::sync::Arc<WarmContainer>,
}

/// Process-global sequence so each container name is unique within a process;
/// combined with the pid it is unique across the host for the process lifetime.
static CONTAINER_SEQ: AtomicU64 = AtomicU64::new(0);

/// Process-global sequence making each background marker token unique.
static MARKER_SEQ: AtomicU64 = AtomicU64::new(0);

/// Prefix of the unique marker token embedded in background command lines so an
/// in-container `pkill -f` can target exactly that exec.
const MARKER_PREFIX: &str = "CODEL00P_DOCKER_BG";

impl DockerBackend {
    /// Build a backend that mounts `workspace_root` and runs commands per
    /// `config`. `workspace_root` must be an absolute host path.
    pub fn new(workspace_root: impl Into<PathBuf>, config: DockerConfig) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            config,
            availability: std::sync::Arc::new(OnceLock::new()),
            warm: std::sync::Arc::new(WarmContainer::default()),
        }
    }

    /// The configuration this backend was built with.
    pub fn config(&self) -> &DockerConfig {
        &self.config
    }

    /// The workspace root this backend mounts into containers.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Generate a unique container name for one invocation/backend.
    fn next_container_name() -> String {
        let seq = CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("codel00p-{}-{}", std::process::id(), seq)
    }

    /// Generate a unique marker token for one background spawn.
    fn next_marker() -> String {
        let seq = MARKER_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("{MARKER_PREFIX}_{}_{seq}", std::process::id())
    }

    /// Verify the `docker` CLI exists and the daemon is reachable, exactly once.
    /// Returns a clear error otherwise so a misconfigured backend fails loudly
    /// rather than with a cryptic "No such file or directory" spawn error.
    fn ensure_available(&self) -> Result<(), HarnessError> {
        let result = self.availability.get_or_init(|| {
            let output = Command::new("docker")
                .arg("version")
                .arg("--format")
                .arg("{{.Server.Version}}")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match output {
                Err(error) => Err(format!(
                    "the `docker` CLI could not be launched: {error}. Install Docker or set agent.execution_backend back to `local`."
                )),
                Ok(out) if out.status.success() => Ok(()),
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    Err(format!(
                        "the Docker daemon is not reachable: {}. Start Docker or set agent.execution_backend back to `local`.",
                        stderr.trim()
                    ))
                }
            }
        });
        result.clone().map_err(|message| HarnessError::ToolFailed {
            name: TOOL_NAME.to_string(),
            message: format!("docker backend selected but `docker` is not available: {message}"),
        })
    }

    /// Build the ephemeral `docker run --rm ...` argv for `spec`.
    fn build_run_args(
        &self,
        spec: &CommandSpec,
        container_name: &str,
    ) -> Result<Vec<String>, HarnessError> {
        build_docker_run_args(
            &self.config,
            &self.workspace_root,
            spec,
            container_name,
            host_uid_gid(),
        )
    }

    /// Ensure the warm container exists, starting it lazily on first use, and
    /// return its name. Idempotent: only the first caller creates it.
    fn ensure_warm_container(&self) -> Result<String, HarnessError> {
        let mut guard = self.warm.name.lock().expect("warm container mutex");
        if let Some(name) = guard.as_ref() {
            return Ok(name.clone());
        }
        // Best-effort reap of warm containers left by DEAD processes before we
        // create our own (crash-safety; conservative — only dead pids removed).
        reap_stale_warm_containers();

        let name = Self::next_container_name();
        let args =
            build_docker_create_args(&self.config, &self.workspace_root, &name, host_uid_gid());
        let output = Command::new("docker")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(spawn_error)?;
        if !output.status.success() {
            return Err(HarnessError::ToolFailed {
                name: TOOL_NAME.to_string(),
                message: format!(
                    "failed to start warm Docker container: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        *guard = Some(name.clone());
        Ok(name)
    }

    /// Map a command `cwd` to its container path.
    fn container_cwd(&self, spec: &CommandSpec) -> Result<PathBuf, HarnessError> {
        map_cwd_to_container(
            &self.workspace_root,
            &self.config.container_mount,
            &spec.cwd,
        )
    }

    // --- warm (exec) execution ------------------------------------------------

    fn run_foreground_warm(
        &self,
        spec: &CommandSpec,
        limits: OutputLimits,
    ) -> Result<CommandOutcome, HarnessError> {
        let name = self.ensure_warm_container()?;
        let container_cwd = self.container_cwd(spec)?;
        // Bound the in-container process with `timeout` so it self-terminates
        // even if the local proxy is killed (mirrors the SSH backend). A small
        // grace over the local deadline keeps the local poll loop the backstop.
        let timeout_secs = limits.timeout.as_secs().saturating_add(2).max(1);
        let args =
            build_docker_exec_args(&name, &container_cwd, spec, ExecWrap::Timeout(timeout_secs));

        let mut child = Command::new("docker")
            .args(&args)
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
                // The in-container `timeout` bounds the process independently;
                // also reap the local proxy now.
                let _ = child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        let output = child.wait_with_output()?;
        let stdout: CappedOutput = cap_output(&output.stdout, limits.max_output_bytes);
        let stderr: CappedOutput = cap_output(&output.stderr, limits.max_output_bytes);

        // `docker exec` proxies the in-container command's exit status. The
        // in-container `timeout` exits 124 when it fired, which we translate to a
        // timeout outcome (in addition to the local-deadline detection above).
        let exec_code = output.status.code();
        let timed_out = timed_out || exec_code == Some(TIMEOUT_EXIT_CODE);
        let exit_code = if timed_out { None } else { exec_code };
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

    fn spawn_background_warm(
        &self,
        spec: &CommandSpec,
    ) -> Result<Box<dyn ChildHandle>, HarnessError> {
        let name = self.ensure_warm_container()?;
        let container_cwd = self.container_cwd(spec)?;
        // Embed a unique marker in the command line so `kill` can `pkill -f` the
        // exact in-container process group without touching the shared container.
        let marker = Self::next_marker();
        let args = build_docker_exec_args(
            &name,
            &container_cwd,
            spec,
            ExecWrap::Marker(marker.clone()),
        );

        let child = Command::new("docker")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(spawn_error)?;

        Ok(Box::new(DockerExecChildHandle {
            child,
            container_name: name,
            marker,
        }))
    }
}

impl TerminalBackend for DockerBackend {
    // Filesystem ops: the workspace is bind-mounted from the host, so the bytes
    // on disk live at the host `workspace_root`. The fs ops are therefore the
    // same workspace-bounded local fs as `LocalBackend`, sharing one impl and
    // one security boundary.

    fn read_file(&self, rel: &Path) -> Result<Vec<u8>, HarnessError> {
        local_fs::read_file(&self.workspace_root, rel)
    }

    fn write_file(&self, rel: &Path, contents: &[u8]) -> Result<(), HarnessError> {
        local_fs::write_file(&self.workspace_root, rel, contents)
    }

    fn delete_file(&self, rel: &Path) -> Result<(), HarnessError> {
        local_fs::delete_file(&self.workspace_root, rel)
    }

    fn create_dir_all(&self, rel: &Path) -> Result<(), HarnessError> {
        local_fs::create_dir_all(&self.workspace_root, rel)
    }

    fn metadata(&self, rel: &Path) -> Result<FileKind, HarnessError> {
        local_fs::metadata(&self.workspace_root, rel)
    }

    fn read_dir(&self, rel: &Path) -> Result<Vec<DirEntry>, HarnessError> {
        local_fs::read_dir(&self.workspace_root, rel)
    }

    /// Override the default walk with the efficient absolute-path local walk:
    /// the workspace is bind-mounted from the host, so traversal reads the host
    /// bytes directly, identical to [`LocalBackend`](super::LocalBackend).
    fn walk(
        &self,
        rel: &Path,
        include_ignored: bool,
        visit: &mut dyn FnMut(&str),
    ) -> Result<(), HarnessError> {
        local_fs::walk_rel(&self.workspace_root, rel, include_ignored, visit)
    }

    fn run_foreground(
        &self,
        spec: &CommandSpec,
        limits: OutputLimits,
    ) -> Result<CommandOutcome, HarnessError> {
        self.ensure_available()?;
        if self.config.reuse_container {
            return self.run_foreground_warm(spec, limits);
        }

        // Ephemeral path: a fresh `docker run --rm` per command.
        let name = Self::next_container_name();
        let args = self.build_run_args(spec, &name)?;

        let mut child = Command::new("docker")
            .args(&args)
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
                // Stop the container itself (not just the local proxy), then
                // kill the proxy. `--rm` removes the container once stopped, so
                // nothing is orphaned.
                docker_kill(&name);
                let _ = child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }

        let output = child.wait_with_output()?;
        let stdout: CappedOutput = cap_output(&output.stdout, limits.max_output_bytes);
        let stderr: CappedOutput = cap_output(&output.stderr, limits.max_output_bytes);
        // `docker run` proxies the container command's exit status as its own,
        // so `output.status.code()` is the in-container exit code.
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
        self.ensure_available()?;
        if self.config.reuse_container {
            return self.spawn_background_warm(spec);
        }

        // Ephemeral path: a fresh `docker run --rm` per command.
        let name = Self::next_container_name();
        let args = self.build_run_args(spec, &name)?;

        let child = Command::new("docker")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(spawn_error)?;

        Ok(Box::new(DockerChildHandle {
            child,
            container_name: name,
        }))
    }

    /// Docker runs commands inside a container, isolated from the host.
    fn is_isolated(&self) -> bool {
        true
    }
}

/// Shared, lazily-populated handle to the warm container. Its `Drop` removes the
/// container, so the container's lifetime matches the backend's (last clone).
#[derive(Debug, Default)]
struct WarmContainer {
    /// The started container's name, or `None` until lazily created (or when
    /// `reuse_container` is false and it is never used).
    name: Mutex<Option<String>>,
}

impl Drop for WarmContainer {
    fn drop(&mut self) {
        // Best-effort teardown: forcibly remove the warm container if we started
        // one. `docker rm -f` both stops and removes it. Errors are ignored (it
        // may already be gone). This runs when the last `Arc<WarmContainer>`
        // clone is dropped, i.e. when the harness is done with the backend.
        if let Ok(guard) = self.name.lock()
            && let Some(name) = guard.as_ref()
        {
            docker_rm_f(name);
        }
    }
}

/// A [`ChildHandle`] for the EPHEMERAL path: a local `docker run` proxy plus the
/// name of the container it fronts. `kill` stops the *container*, which both
/// ends the command and closes the proxied pipes so reader threads finish.
struct DockerChildHandle {
    child: Child,
    container_name: String,
}

impl ChildHandle for DockerChildHandle {
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
        // Stop the container so the in-container process actually dies and the
        // proxied stdout/stderr close; `--rm` then removes the container. Then
        // reap the local proxy. Killing only the proxy would leave the
        // container running.
        docker_kill(&self.container_name);
        let _ = self.child.kill();
        Ok(())
    }
}

/// A [`ChildHandle`] for the WARM path: a local `docker exec` proxy plus the
/// shared container name and the unique marker embedded in the in-container
/// command line. `kill` terminates ONLY that one in-container process group via
/// `docker exec <name> pkill -TERM -f <marker>` — the shared container survives.
///
/// Residual caveat: if the image lacks `pkill` (uncommon on Linux, e.g. a
/// distroless image without procps/BusyBox), `kill` falls back to closing the
/// local channel only, which may leave the in-container process running until it
/// finishes on its own. `alpine` (BusyBox) and most full images ship `pkill`.
struct DockerExecChildHandle {
    child: Child,
    container_name: String,
    marker: String,
}

impl ChildHandle for DockerExecChildHandle {
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
        // Terminate the in-container process group carrying our unique marker
        // (leaving the shared container alive), then reap the local proxy.
        docker_pkill_marker(&self.container_name, &self.marker);
        let _ = self.child.kill();
        Ok(())
    }
}

/// Best-effort `docker kill <name>` (ephemeral path). Errors are ignored: the
/// container may have already exited (and been removed by `--rm`).
fn docker_kill(name: &str) {
    let _ = Command::new("docker")
        .arg("kill")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Best-effort `docker rm -f <name>` (warm-path teardown). Stops + removes.
fn docker_rm_f(name: &str) {
    let _ = Command::new("docker")
        .arg("rm")
        .arg("-f")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Best-effort `docker exec <name> pkill -TERM -f <marker>` (warm background
/// kill). SIGTERMs the in-container process whose command line carries `marker`.
fn docker_pkill_marker(name: &str, marker: &str) {
    let _ = Command::new("docker")
        .arg("exec")
        .arg(name)
        .arg("pkill")
        .arg("-TERM")
        .arg("-f")
        .arg(marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Best-effort reaper for warm containers left behind by DEAD processes (Drop
/// did not run, e.g. SIGKILL). Lists all warm containers and their pid label,
/// and `docker rm -f`s only those whose recorded pid is no longer running. This
/// is conservative — a container from a still-running pid (possibly ours or a
/// concurrent codel00p) is never touched.
fn reap_stale_warm_containers() {
    // List `<id> <pid>` per warm container in one call. NB: do NOT pass `-q`
    // here — `--quiet` and `--format` conflict and Docker silently ignores the
    // format (printing bare IDs), which would defeat the pid check.
    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--no-trunc",
            "--filter",
            &format!("label={LABEL_KIND}"),
            "--format",
            &format!("{{{{.ID}}}} {{{{.Label \"{LABEL_PID_KEY}\"}}}}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else { return };
    if !output.status.success() {
        return;
    }
    for (id, pid) in parse_warm_ps(&String::from_utf8_lossy(&output.stdout)) {
        // Only reap containers whose recorded pid is gone (dead process).
        if let Some(pid) = pid
            && !pid_is_alive(pid)
        {
            docker_rm_f(&id);
        }
    }
}

/// Parse the reaper's `docker ps` output (one `<id> <pid>` per line) into
/// `(id, Some(pid))` pairs. A line without a parseable pid yields `(id, None)`
/// (and is left untouched by the reaper, conservatively).
fn parse_warm_ps(stdout: &str) -> Vec<(String, Option<u32>)> {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.split_whitespace();
            let id = parts.next()?.to_string();
            let pid = parts.next().and_then(|p| p.parse::<u32>().ok());
            Some((id, pid))
        })
        .collect()
}

/// Whether a process with `pid` is currently alive (Unix: `kill(pid, 0)`). On
/// non-Unix we conservatively report `true` so the reaper never removes a
/// container it cannot prove is stale.
fn pid_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `kill` with signal 0 performs only an existence/permission
        // check; it sends no signal and has no memory effects.
        let res = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if res == 0 {
            return true;
        }
        // ESRCH ⇒ no such process (dead). Any other errno (e.g. EPERM ⇒ the
        // process exists but we may not signal it) means it is alive.
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

fn spawn_error(error: std::io::Error) -> HarnessError {
    HarnessError::ToolFailed {
        name: TOOL_NAME.to_string(),
        message: format!("failed to launch `docker`: {error}"),
    }
}

/// The host process's real uid/gid, or `None` on non-Unix platforms.
fn host_uid_gid() -> Option<(u32, u32)> {
    #[cfg(unix)]
    {
        // SAFETY: getuid/getgid are always-succeeding syscalls with no
        // preconditions and no memory effects.
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        Some((uid, gid))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Map a host `cwd` (absolute, under `workspace_root`) to the corresponding
/// path inside the container mount. Returns an error if `cwd` is not under the
/// workspace root — which should never happen, since the tool resolves the cwd
/// within the workspace, but we guard it rather than silently mounting the
/// wrong directory.
fn map_cwd_to_container(
    workspace_root: &Path,
    container_mount: &Path,
    cwd: &Path,
) -> Result<PathBuf, HarnessError> {
    let relative = cwd
        .strip_prefix(workspace_root)
        .map_err(|_| HarnessError::ToolFailed {
            name: TOOL_NAME.to_string(),
            message: format!(
                "command working directory {} is not under the Docker workspace root {}",
                cwd.display(),
                workspace_root.display()
            ),
        })?;
    // A cwd equal to the workspace root yields an empty relative path; joining
    // it onto the mount would append a trailing separator (`/workspace/`), so
    // return the mount itself in that case.
    if relative.as_os_str().is_empty() {
        Ok(container_mount.to_path_buf())
    } else {
        Ok(container_mount.join(relative))
    }
}

/// Append the resource/network/user/-e flags shared by `run` and `create` to
/// `args`, in a stable order.
fn push_resource_flags(
    args: &mut Vec<String>,
    config: &DockerConfig,
    host_user: Option<(u32, u32)>,
) {
    if let Some(memory) = &config.memory {
        args.push("--memory".to_string());
        args.push(memory.clone());
    }
    if let Some(cpus) = &config.cpus {
        args.push("--cpus".to_string());
        args.push(cpus.clone());
    }
    if let Some(network) = &config.network {
        args.push("--network".to_string());
        args.push(network.clone());
    }
    if config.map_host_user
        && let Some((uid, gid)) = host_user
    {
        args.push("--user".to_string());
        args.push(format!("{uid}:{gid}"));
    }
    for (key, value) in &config.env {
        args.push("-e".to_string());
        args.push(format!("{key}={value}"));
    }
}

/// Build the EPHEMERAL `docker run --rm ...` argv (everything after `docker`)
/// for one command. Pure so the argv composition is fully unit-testable.
///
/// Layout: `run --rm --name <name> [--memory M] [--cpus C] [--network N]
/// [--user uid:gid] [-e K=V ...] -v <host>:<mount> -w <container_cwd> <image>
/// <program> [args...]`.
fn build_docker_run_args(
    config: &DockerConfig,
    workspace_root: &Path,
    spec: &CommandSpec,
    container_name: &str,
    host_user: Option<(u32, u32)>,
) -> Result<Vec<String>, HarnessError> {
    let container_cwd = map_cwd_to_container(workspace_root, &config.container_mount, &spec.cwd)?;

    let mut args: Vec<String> = vec![
        "run".to_string(),
        "--rm".to_string(),
        "--name".to_string(),
        container_name.to_string(),
    ];

    push_resource_flags(&mut args, config, host_user);

    // Bind-mount the workspace root at the configured container mount.
    args.push("-v".to_string());
    args.push(format!(
        "{}:{}",
        workspace_root.display(),
        config.container_mount.display()
    ));

    // Working directory inside the container is the mapped cwd.
    args.push("-w".to_string());
    args.push(container_cwd.display().to_string());

    // Image, then the command and its arguments.
    args.push(config.image.clone());
    args.push(spec.program.clone());
    args.extend(spec.args.iter().cloned());

    Ok(args)
}

/// Build the WARM `docker run -d ...` create argv (everything after `docker`)
/// for the long-lived container. Pure so the argv composition is unit-testable.
///
/// Layout: `run -d --name <name> --label codel00p.kind=warm --label
/// codel00p.pid=<pid> [--memory M] [--cpus C] [--network N] [--user uid:gid]
/// [-e K=V ...] -v <host>:<mount> -w <mount> <image> <keepalive...>`.
///
/// Resource limits, network, user, and the bind mount are applied ONCE here at
/// create time (not per command). The working directory is set to the mount
/// root; per-command cwds are applied via `docker exec -w` instead.
fn build_docker_create_args(
    config: &DockerConfig,
    workspace_root: &Path,
    container_name: &str,
    host_user: Option<(u32, u32)>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        container_name.to_string(),
        "--label".to_string(),
        LABEL_KIND.to_string(),
        "--label".to_string(),
        format!("{LABEL_PID_KEY}={}", std::process::id()),
    ];

    push_resource_flags(&mut args, config, host_user);

    args.push("-v".to_string());
    args.push(format!(
        "{}:{}",
        workspace_root.display(),
        config.container_mount.display()
    ));
    // Default working dir is the mount root; per-command cwd is set on `exec`.
    args.push("-w".to_string());
    args.push(config.container_mount.display().to_string());

    args.push(config.image.clone());
    args.extend(KEEPALIVE.iter().map(|s| s.to_string()));

    args
}

/// How a warm `docker exec` command's payload is wrapped.
enum ExecWrap {
    /// Foreground: wrap with the in-container `timeout <secs>` so the process
    /// self-bounds remotely. The program/args follow as verbatim argv (no
    /// shell), matching the ephemeral `docker run` path's quoting exactly.
    Timeout(u64),
    /// Background: run inside a shell supervisor that carries the unique marker
    /// as its `$0` (so it stays in the in-container command line for `pkill -f`)
    /// and forwards SIGTERM to the actual program. See [`build_marker_script`].
    Marker(String),
}

/// Build the background supervisor shell script. The script runs the program as
/// a child, records its pid, traps SIGTERM to forward it to that child, then
/// waits. Run as `sh -c <script> <marker> <program> <args...>`, the marker
/// becomes `$0` — visible in the in-container command line so a later
/// `pkill -TERM -f <marker>` matches THIS supervisor (and only it). The trap
/// then propagates the signal to the actual program. Because the supervisor does
/// not `exec`, the marker stays in its command line for the whole lifetime.
fn build_marker_script() -> String {
    // "$@" is the program + its args (positional params after $0=marker).
    // On TERM we kill the recorded child pid and exit; otherwise we wait for it
    // and propagate its exit status.
    "\"$@\" & p=$!; trap 'kill -TERM $p 2>/dev/null' TERM; wait $p".to_string()
}

/// Build the `docker exec ...` argv (everything after `docker`) running `spec`'s
/// program in the warm container `name` at `container_cwd`. Pure and
/// unit-testable.
///
/// Foreground layout (verbatim argv, no shell — quoting matches the ephemeral
/// `docker run` path exactly): `exec -w <cwd> <name> timeout <secs> <program>
/// [args...]`.
///
/// Background layout (shell supervisor carrying the marker as `$0`):
/// `exec -w <cwd> <name> sh -c <script> <marker> <program> [args...]`.
fn build_docker_exec_args(
    name: &str,
    container_cwd: &Path,
    spec: &CommandSpec,
    wrap: ExecWrap,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "exec".to_string(),
        "-w".to_string(),
        container_cwd.display().to_string(),
        name.to_string(),
    ];

    match wrap {
        ExecWrap::Timeout(secs) => {
            args.push("timeout".to_string());
            args.push(secs.to_string());
            args.push(spec.program.clone());
            args.extend(spec.args.iter().cloned());
        }
        ExecWrap::Marker(marker) => {
            // `sh -c <script> <marker> <program> <args...>`: $0 = marker (kept in
            // the command line for pkill), $@ = program + args.
            args.push("sh".to_string());
            args.push("-c".to_string());
            args.push(build_marker_script());
            args.push(marker);
            args.push(spec.program.clone());
            args.extend(spec.args.iter().cloned());
        }
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> PathBuf {
        PathBuf::from("/home/user/project")
    }

    fn spec_in(cwd: &str) -> CommandSpec {
        CommandSpec::new(
            "sh",
            vec!["-c".to_string(), "echo hi".to_string()],
            PathBuf::from(cwd),
        )
    }

    // === Ephemeral `docker run` argv ========================================

    #[test]
    fn argv_contains_core_run_flags_and_command() {
        let config = DockerConfig::new("alpine");
        let args = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project"),
            "codel00p-1-0",
            Some((501, 20)),
        )
        .unwrap();

        // Leading `run --rm --name <name>`.
        assert_eq!(&args[0..4], &["run", "--rm", "--name", "codel00p-1-0"]);
        // Mount of the workspace root at the default /workspace.
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-v" && w[1] == "/home/user/project:/workspace"),
            "expected workspace bind-mount, got {args:?}"
        );
        // Working dir maps to the mount root for a cwd == workspace root.
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-w" && w[1] == "/workspace")
        );
        // Default network is `none`.
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--network" && w[1] == "none")
        );
        // Image then program then args, in order, at the tail.
        let tail = &args[args.len() - 4..];
        assert_eq!(tail, &["alpine", "sh", "-c", "echo hi"]);
    }

    #[test]
    fn nested_cwd_maps_under_mount() {
        let config = DockerConfig::new("alpine");
        let args = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project/crates/foo"),
            "c",
            None,
        )
        .unwrap();
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-w" && w[1] == "/workspace/crates/foo"),
            "expected nested cwd mapping, got {args:?}"
        );
    }

    #[test]
    fn custom_container_mount_is_honored() {
        let mut config = DockerConfig::new("alpine");
        config.container_mount = PathBuf::from("/src");
        let args = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project/sub"),
            "c",
            None,
        )
        .unwrap();
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-v" && w[1] == "/home/user/project:/src")
        );
        assert!(args.windows(2).any(|w| w[0] == "-w" && w[1] == "/src/sub"));
    }

    #[test]
    fn cwd_outside_workspace_is_an_error() {
        let config = DockerConfig::new("alpine");
        let result = build_docker_run_args(&config, &workspace(), &spec_in("/etc"), "c", None);
        let Err(HarnessError::ToolFailed { message, .. }) = result else {
            panic!("expected an error for cwd outside the workspace");
        };
        assert!(message.contains("not under the Docker workspace root"));
    }

    #[test]
    fn map_host_user_emits_user_flag_only_when_uid_present() {
        let config = DockerConfig::new("alpine");
        let with_user = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project"),
            "c",
            Some((501, 20)),
        )
        .unwrap();
        assert!(
            with_user
                .windows(2)
                .any(|w| w[0] == "--user" && w[1] == "501:20")
        );

        // No uid source (e.g. non-Unix) ⇒ no --user even though map_host_user is on.
        let without_user = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project"),
            "c",
            None,
        )
        .unwrap();
        assert!(!without_user.iter().any(|a| a == "--user"));
    }

    #[test]
    fn map_host_user_disabled_omits_user_flag() {
        let mut config = DockerConfig::new("alpine");
        config.map_host_user = false;
        let args = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project"),
            "c",
            Some((501, 20)),
        )
        .unwrap();
        assert!(!args.iter().any(|a| a == "--user"));
    }

    #[test]
    fn resource_and_env_flags_appear_when_configured() {
        let mut config = DockerConfig::new("rust:1");
        config.memory = Some("512m".to_string());
        config.cpus = Some("1.5".to_string());
        config.network = Some("bridge".to_string());
        config.env = vec![
            ("FOO".to_string(), "bar".to_string()),
            ("BAZ".to_string(), "qux".to_string()),
        ];
        let args = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project"),
            "c",
            None,
        )
        .unwrap();
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--memory" && w[1] == "512m")
        );
        assert!(args.windows(2).any(|w| w[0] == "--cpus" && w[1] == "1.5"));
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--network" && w[1] == "bridge")
        );
        assert!(args.windows(2).any(|w| w[0] == "-e" && w[1] == "FOO=bar"));
        assert!(args.windows(2).any(|w| w[0] == "-e" && w[1] == "BAZ=qux"));
        assert!(args.iter().any(|a| a == "rust:1"));
    }

    #[test]
    fn no_resource_flags_when_unset() {
        let mut config = DockerConfig::new("alpine");
        config.network = None;
        let args = build_docker_run_args(
            &config,
            &workspace(),
            &spec_in("/home/user/project"),
            "c",
            None,
        )
        .unwrap();
        assert!(!args.iter().any(|a| a == "--memory"));
        assert!(!args.iter().any(|a| a == "--cpus"));
        assert!(!args.iter().any(|a| a == "--network"));
    }

    // === Warm `docker run -d` create argv ===================================

    #[test]
    fn create_argv_has_detached_name_labels_mount_and_keepalive() {
        let config = DockerConfig::new("alpine");
        let args = build_docker_create_args(&config, &workspace(), "codel00p-9-0", Some((501, 20)));

        // Leading `run -d --name <name>`.
        assert_eq!(&args[0..4], &["run", "-d", "--name", "codel00p-9-0"]);
        // Both labels present (kind + pid).
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--label" && w[1] == LABEL_KIND),
            "expected kind label, got {args:?}"
        );
        let expected_pid = format!("{LABEL_PID_KEY}={}", std::process::id());
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--label" && w[1] == expected_pid),
            "expected pid label, got {args:?}"
        );
        // Resource flags applied at create time.
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--network" && w[1] == "none")
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--user" && w[1] == "501:20")
        );
        // Bind-mount and default working dir at the mount.
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-v" && w[1] == "/home/user/project:/workspace")
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-w" && w[1] == "/workspace")
        );
        // Image then keepalive at the tail.
        let tail = &args[args.len() - 4..];
        assert_eq!(tail, &["alpine", "tail", "-f", "/dev/null"]);
    }

    #[test]
    fn create_argv_applies_resource_limits_and_env() {
        let mut config = DockerConfig::new("rust:1");
        config.memory = Some("1g".to_string());
        config.cpus = Some("2".to_string());
        config.network = Some("bridge".to_string());
        config.env = vec![("FOO".to_string(), "bar".to_string())];
        let args = build_docker_create_args(&config, &workspace(), "c", None);
        assert!(args.windows(2).any(|w| w[0] == "--memory" && w[1] == "1g"));
        assert!(args.windows(2).any(|w| w[0] == "--cpus" && w[1] == "2"));
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--network" && w[1] == "bridge")
        );
        assert!(args.windows(2).any(|w| w[0] == "-e" && w[1] == "FOO=bar"));
        // No --user when no uid source.
        assert!(!args.iter().any(|a| a == "--user"));
    }

    // === Warm `docker exec` argv ============================================

    #[test]
    fn exec_argv_foreground_wraps_with_timeout() {
        let args = build_docker_exec_args(
            "codel00p-9-0",
            Path::new("/workspace/crates/foo"),
            &spec_in("/home/user/project/crates/foo"),
            ExecWrap::Timeout(12),
        );
        // `exec -w <cwd> <name> timeout 12 <program> <args>`.
        assert_eq!(
            args,
            vec![
                "exec",
                "-w",
                "/workspace/crates/foo",
                "codel00p-9-0",
                "timeout",
                "12",
                "sh",
                "-c",
                "echo hi",
            ]
        );
    }

    #[test]
    fn exec_argv_background_wraps_in_marker_supervisor() {
        let args = build_docker_exec_args(
            "codel00p-9-0",
            Path::new("/workspace"),
            &spec_in("/home/user/project"),
            ExecWrap::Marker("CODEL00P_DOCKER_BG_42_7".to_string()),
        );
        // `exec -w <cwd> <name> sh -c <script> <marker> <program> <args...>`.
        assert_eq!(
            args,
            vec![
                "exec",
                "-w",
                "/workspace",
                "codel00p-9-0",
                "sh",
                "-c",
                &build_marker_script(),
                "CODEL00P_DOCKER_BG_42_7",
                "sh",
                "-c",
                "echo hi",
            ]
        );
        // The marker is positioned as `$0` (immediately after the script), so it
        // lands in the supervisor's in-container command line for `pkill -f`.
        let script_idx = args
            .iter()
            .position(|a| a == &build_marker_script())
            .unwrap();
        assert_eq!(args[script_idx + 1], "CODEL00P_DOCKER_BG_42_7");
    }

    // === Config / naming / parsing ==========================================

    #[test]
    fn config_defaults_are_sane() {
        let config = DockerConfig::default();
        assert_eq!(config.image, "alpine");
        assert_eq!(config.container_mount, PathBuf::from("/workspace"));
        assert_eq!(config.network.as_deref(), Some("none"));
        assert!(config.map_host_user);
        assert!(config.memory.is_none());
        assert!(config.cpus.is_none());
        assert!(config.env.is_empty());
        // The warm container optimization is on by default (the perf win).
        assert!(config.reuse_container);
    }

    #[test]
    fn container_names_are_unique() {
        let a = DockerBackend::next_container_name();
        let b = DockerBackend::next_container_name();
        assert_ne!(a, b);
        assert!(a.starts_with("codel00p-"));
    }

    #[test]
    fn markers_are_unique_and_prefixed() {
        let a = DockerBackend::next_marker();
        let b = DockerBackend::next_marker();
        assert_ne!(a, b);
        assert!(a.starts_with(MARKER_PREFIX));
    }

    #[test]
    fn parse_warm_ps_extracts_id_and_pid() {
        let out = "abc123 1000\ndef456 2000\nnopid \nghi789\n";
        let parsed = parse_warm_ps(out);
        assert_eq!(
            parsed,
            vec![
                ("abc123".to_string(), Some(1000)),
                ("def456".to_string(), Some(2000)),
                ("nopid".to_string(), None),
                ("ghi789".to_string(), None),
            ]
        );
    }

    #[test]
    fn pid_is_alive_for_self_and_dead_for_bogus() {
        // Our own pid is alive.
        assert!(pid_is_alive(std::process::id()));
        // A very large pid is essentially guaranteed not to exist on Unix; the
        // reaper treats it as dead. (On non-Unix this is always-true by design,
        // so only assert the negative on Unix.)
        #[cfg(unix)]
        assert!(!pid_is_alive(4_000_000_000));
    }
}
