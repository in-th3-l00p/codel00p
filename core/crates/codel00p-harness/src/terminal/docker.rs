//! Docker terminal backend — Phase 2 of initiative #7.
//!
//! [`DockerBackend`] satisfies [`TerminalBackend`](super::TerminalBackend) by
//! running each command in its own ephemeral container (`docker run --rm`) with
//! the agent's workspace bind-mounted. It mirrors [`LocalBackend`](super::LocalBackend)
//! exactly where it can — same `Stdio` wiring, same timeout poll loop, same
//! output capping, same exit-code/`timed_out` semantics — and adds the container
//! lifecycle guarantees on top:
//!
//! * **Per-command container, ephemeral.** Each invocation is a fresh
//!   `docker run --rm --name codel00p-<pid>-<seq> ...`. `--rm` removes the
//!   container the moment its process exits, so nothing is left behind on the
//!   happy path.
//! * **Workspace bind-mount.** The workspace root (an absolute host path) is
//!   mounted at [`DockerConfig::container_mount`] (default `/workspace`). The
//!   command's host `cwd` (already resolved within the workspace) is mapped to
//!   the container path by re-rooting it under the mount. Edits persist on the
//!   host because the mount is a bind mount.
//! * **No orphaned containers on timeout/kill.** The local `docker run` process
//!   is a *proxy*; killing it alone can leave the container running. So on
//!   timeout (foreground) and on [`ChildHandle::kill`] (background) we also run
//!   `docker kill <name>`, which stops the container and — because of `--rm` —
//!   removes it. Killing the container closes the proxied stdout/stderr, so the
//!   background reader threads hit EOF and finish, preserving the
//!   "exited ⇒ all output buffered" invariant.
//! * **Host file ownership.** On Unix, when [`DockerConfig::map_host_user`] is
//!   set (the default), the container runs as `--user <uid>:<gid>` using the
//!   host process's real uid/gid, so files it creates in the bind-mounted
//!   workspace are owned by the host user rather than root.
//! * **Graceful degradation.** Before the first run the backend checks that the
//!   `docker` CLI exists and the daemon is reachable (`docker version`), and
//!   returns a clear [`HarnessError`] rather than a cryptic spawn failure if not.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        OnceLock,
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

/// Runs commands inside ephemeral Docker containers with the workspace mounted.
///
/// Cheap to clone (the config is small and the workspace root is a `PathBuf`),
/// so it can live behind an `Arc<dyn TerminalBackend>` like [`LocalBackend`].
#[derive(Clone, Debug)]
pub struct DockerBackend {
    /// Absolute host path of the workspace root (mounted into the container).
    workspace_root: PathBuf,
    config: DockerConfig,
    /// One-time `docker version` availability result, cached after first check.
    availability: std::sync::Arc<OnceLock<Result<(), String>>>,
}

/// Process-global sequence so each container name is unique within a process;
/// combined with the pid it is unique across the host for the process lifetime.
static CONTAINER_SEQ: AtomicU64 = AtomicU64::new(0);

impl DockerBackend {
    /// Build a backend that mounts `workspace_root` and runs commands per
    /// `config`. `workspace_root` must be an absolute host path.
    pub fn new(workspace_root: impl Into<PathBuf>, config: DockerConfig) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            config,
            availability: std::sync::Arc::new(OnceLock::new()),
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

    /// Generate a unique container name for one invocation.
    fn next_container_name() -> String {
        let seq = CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("codel00p-{}-{}", std::process::id(), seq)
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

    /// Build the full `docker run` argv (everything after the `docker`
    /// executable) for `spec`. Pure given `(spec, config, workspace_root,
    /// container_name)` so it is unit-testable without invoking Docker.
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

    /// Docker runs each command in its own container, isolated from the host.
    fn is_isolated(&self) -> bool {
        true
    }
}

/// A [`ChildHandle`] backed by a local `docker run` proxy process plus the name
/// of the container it fronts. `kill` stops the *container*, which both ends the
/// command and closes the proxied pipes so reader threads finish.
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

/// Best-effort `docker kill <name>`. Errors are ignored: the container may have
/// already exited (and been removed by `--rm`), in which case `docker kill`
/// failing is expected and harmless.
fn docker_kill(name: &str) {
    let _ = Command::new("docker")
        .arg("kill")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn spawn_error(error: std::io::Error) -> HarnessError {
    HarnessError::ToolFailed {
        name: TOOL_NAME.to_string(),
        message: format!("failed to launch `docker run`: {error}"),
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

/// Build the `docker run` argv (everything after `docker`) for one command.
/// Factored out and pure so the argv composition is fully unit-testable.
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
    }

    #[test]
    fn container_names_are_unique() {
        let a = DockerBackend::next_container_name();
        let b = DockerBackend::next_container_name();
        assert_ne!(a, b);
        assert!(a.starts_with("codel00p-"));
    }
}
