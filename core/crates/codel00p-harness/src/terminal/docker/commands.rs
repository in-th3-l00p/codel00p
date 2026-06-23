//! The pure argument/command-building layer for the Docker backend: mapping the
//! host cwd into the container, resource-limit flags, the `docker run`/`create`/
//! `exec` argument vectors, and the background marker script. Deterministic
//! string/arg construction with no process spawning — `super::DockerBackend`
//! orchestrates these into actual `docker` invocations.

use super::*;

/// Map a host `cwd` (absolute, under `workspace_root`) to the corresponding
/// path inside the container mount. Returns an error if `cwd` is not under the
/// workspace root — which should never happen, since the tool resolves the cwd
/// within the workspace, but we guard it rather than silently mounting the
/// wrong directory.
pub(super) fn map_cwd_to_container(
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
pub(super) fn push_resource_flags(
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
pub(super) fn build_docker_run_args(
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
pub(super) fn build_docker_create_args(
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
pub(super) enum ExecWrap {
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
pub(super) fn build_marker_script() -> String {
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
pub(super) fn build_docker_exec_args(
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
