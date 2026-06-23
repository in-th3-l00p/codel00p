//! The pure shell-command layer for the SSH backend: building the remote command
//! strings (exec, background, kill, and each filesystem op), path mapping and
//! safety checks, and parsing remote stdout (metadata, dir listings, walks).
//!
//! Everything here is a deterministic string function with no process spawning or
//! network — which is what makes it exhaustively unit-testable and keeps
//! `super::SshBackend` a thin orchestration layer over `ssh`/`scp`.

use super::*;

pub(super) fn next_marker() -> String {
    let seq = MARKER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{MARKER_PREFIX}_{}_{seq}", std::process::id())
}

/// Single-quote-escape `s` for a POSIX shell. Wraps the string in single quotes
/// and replaces every embedded single quote with the `'\''` idiom (close quote,
/// escaped quote, reopen quote). This is safe for ALL inputs — spaces, `$`,
/// backticks, `;`, `&&`, newlines, and quotes — because inside single quotes the
/// shell treats every character literally.
pub(crate) fn sh_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Build the base ssh options shared by every invocation: batch/quiet-ish flags,
/// connection multiplexing, port, identity, and any user-supplied `-o`s. The
/// target and the `--`/command are appended by callers.
pub(super) fn build_base_ssh_options(
    config: &SshConfig,
    control_socket: Option<&Path>,
) -> Vec<String> {
    let mut opts: Vec<String> = Vec::new();

    // Connection multiplexing: reuse one TCP/auth connection across the many
    // per-op invocations. Without a socket path (only in unit tests) we skip it.
    if let Some(socket) = control_socket {
        opts.push("-o".to_string());
        opts.push("ControlMaster=auto".to_string());
        opts.push("-o".to_string());
        opts.push(format!("ControlPath={}", socket.display()));
        opts.push("-o".to_string());
        opts.push(format!("ControlPersist={CONTROL_PERSIST_SECS}"));
    }

    if let Some(port) = config.port {
        opts.push("-p".to_string());
        opts.push(port.to_string());
    }
    if let Some(identity) = &config.identity_file {
        opts.push("-i".to_string());
        opts.push(identity.display().to_string());
    }
    for option in &config.options {
        opts.push("-o".to_string());
        opts.push(option.clone());
    }
    opts
}

/// Build the `cd <cwd> && exec <program> <args...>` remote command string for a
/// foreground/exec run. Every component is shell-quoted. `exec` replaces the
/// shell so the program's own exit status is what ssh proxies.
pub(super) fn build_exec_command(remote_cwd: &str, spec: &CommandSpec) -> String {
    let mut command = format!(
        "cd {} && exec {}",
        sh_quote(remote_cwd),
        sh_quote(&spec.program)
    );
    for arg in &spec.args {
        command.push(' ');
        command.push_str(&sh_quote(arg));
    }
    command
}

/// Wrap a remote command in GNU `timeout <secs> sh -c '<command>'` so the remote
/// process is bounded and killed remotely. `timeout` exits 124 when it fires.
pub(super) fn wrap_with_timeout(inner: &str, secs: u64) -> String {
    format!("timeout {secs} sh -c {}", sh_quote(inner))
}

/// Build the background remote command: run in a new session (`setsid`, its own
/// process group) so the whole group can be reaped at once, with the unique
/// `marker` embedded LITERALLY in the command line (as an inert leading
/// assignment to a `sh -c` positional) so a later `pkill -f <marker>` finds it
/// regardless of who owns the stdout pipe. The program `exec`s so its
/// stdout/stderr proxy through ssh.
pub(super) fn build_background_command(
    remote_cwd: &str,
    spec: &CommandSpec,
    marker: &str,
) -> String {
    let mut program = format!("exec {}", sh_quote(&spec.program));
    for arg in &spec.args {
        program.push(' ');
        program.push_str(&sh_quote(arg));
    }
    // `setsid sh -c <script> <marker>` runs the script in a fresh session; the
    // marker becomes `$0` of the inner shell — visible in the process command
    // line for `pkill -f`, but never executed.
    format!(
        "cd {} && exec setsid sh -c {} {}",
        sh_quote(remote_cwd),
        sh_quote(&program),
        sh_quote(marker),
    )
}

/// Build the remote kill command: SIGTERM every process whose command line
/// contains the unique `marker`. Under `setsid` the matched leader's session
/// children die with it. Tolerant if nothing matches (already exited).
pub(super) fn build_kill_command(marker: &str) -> String {
    format!("pkill -TERM -f {} 2>/dev/null; true", sh_quote(marker))
}

/// Build the `cat`-with-boundary-check remote command for `read_file`.
/// Resolves the real path (`readlink -f`) and requires it to stay under the
/// remote workspace before reading, mirroring local's canonicalize+prefix check.
pub(super) fn build_read_file_command(workspace: &Path, rpath: &str) -> String {
    let ws = workspace_str(workspace);
    // `readlink -f` resolves symlinks; the case-statement enforces containment
    // (the real path must start with "<ws>/" or equal "<ws>"). On escape we exit
    // nonzero so the caller surfaces a WorkspaceEscape-equivalent error.
    format!(
        "real=$(readlink -f {p}) && case \"$real/\" in {ws_prefix}) cat -- {p};; *) echo 'workspace escape' 1>&2; exit 1;; esac",
        p = sh_quote(rpath),
        ws_prefix = boundary_glob(&ws),
    )
}

/// Build the `mkdir -p && cat > file` remote command for `write_file` (binary
/// safe via stdin). Creates parent dirs first.
pub(super) fn build_write_file_command(rpath: &str) -> String {
    format!(
        "mkdir -p -- {parent} && cat > {p}",
        parent = sh_quote(&remote_parent(rpath)),
        p = sh_quote(rpath),
    )
}

/// Build the `delete_file` remote command: require the file to exist (matching
/// local `delete_file`), boundary-check the resolved path, then `rm`.
pub(super) fn build_delete_file_command(workspace: &Path, rpath: &str) -> String {
    let ws = workspace_str(workspace);
    format!(
        "test -e {p} && real=$(readlink -f {p}) && case \"$real/\" in {ws_prefix}) rm -f -- {p};; *) echo 'workspace escape' 1>&2; exit 1;; esac",
        p = sh_quote(rpath),
        ws_prefix = boundary_glob(&ws),
    )
}

/// Build the `mkdir -p` remote command for `create_dir_all`.
pub(super) fn build_mkdir_command(rpath: &str) -> String {
    format!("mkdir -p -- {}", sh_quote(rpath))
}

/// Build the `stat`-based metadata command. Prints `<type>|<size>` for an
/// existing path and nothing (success, empty) for a missing one, so the parser
/// can map a missing path to [`FileKind::MISSING`] without an error. Includes the
/// symlink-boundary check.
pub(super) fn build_metadata_command(workspace: &Path, rpath: &str) -> String {
    let ws = workspace_str(workspace);
    // If the path does not exist, succeed with empty output (MISSING). If it
    // exists, boundary-check the resolved real path, then `stat -c '%F|%s'`.
    format!(
        "if [ ! -e {p} ] && [ ! -L {p} ]; then exit 0; fi; real=$(readlink -f {p}) && case \"$real/\" in {ws_prefix}) stat -c '%F|%s' -- {p};; *) echo 'workspace escape' 1>&2; exit 1;; esac",
        p = sh_quote(rpath),
        ws_prefix = boundary_glob(&ws),
    )
}

/// Build the `read_dir` command: list immediate entries with a type marker.
/// Emits `<y> <name>` per line where `<y>` is `find`'s type letter (`d`/`f`/…).
pub(super) fn build_read_dir_command(workspace: &Path, rpath: &str) -> String {
    let ws = workspace_str(workspace);
    format!(
        "real=$(readlink -f {p}) && case \"$real/\" in {ws_prefix}) find {p} -maxdepth 1 -mindepth 1 -printf '%y %f\\n';; *) echo 'workspace escape' 1>&2; exit 1;; esac",
        p = sh_quote(rpath),
        ws_prefix = boundary_glob(&ws),
    )
}

/// Build the single-`find` walk command. Prunes the default ignored dirs unless
/// `include_ignored`, prints absolute file paths (the parser strips the
/// workspace prefix). The boundary is implicit: the search root is already under
/// the workspace and `find` does not follow symlinks by default.
pub(super) fn build_walk_command(rpath: &str, include_ignored: bool) -> String {
    if include_ignored {
        return format!("find {} -type f", sh_quote(rpath));
    }
    // Prune each ignored directory name anywhere in the tree, then print files.
    // Layout: find <root> \( -type d \( -name a -o -name b ... \) -prune \) -o -type f -print
    let mut prune = String::new();
    for (i, name) in DEFAULT_IGNORED_DIRS.iter().enumerate() {
        if i > 0 {
            prune.push_str(" -o ");
        }
        prune.push_str(&format!("-name {}", sh_quote(name)));
    }
    format!(
        "find {root} \\( -type d \\( {prune} \\) -prune \\) -o -type f -print",
        root = sh_quote(rpath),
    )
}

/// The `case`-glob alternatives enforcing "resolved path is inside the
/// workspace": either exactly `<ws>/` or under `<ws>/…`. The workspace string is
/// shell-quoted; we append the glob `*` outside the quotes so it stays a glob.
pub(super) fn boundary_glob(ws: &str) -> String {
    // `"<ws>/"` matches the workspace root itself (real + trailing slash);
    // `"<ws>/"*` matches anything beneath it.
    format!("{q}/ | {q}/*", q = sh_quote(ws))
}

/// The remote parent directory of an absolute remote path (string-level, since
/// the path is remote and not on the local fs). Returns `/` for a top-level
/// path and the path itself if it has no `/` (shouldn't happen for absolutes).
pub(super) fn remote_parent(rpath: &str) -> String {
    match rpath.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => rpath[..idx].to_string(),
        None => ".".to_string(),
    }
}

/// The workspace path as a `/`-normalized string with no trailing slash.
pub(super) fn workspace_str(workspace: &Path) -> String {
    let s = workspace.to_string_lossy().replace('\\', "/");
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Map a command `cwd` (absolute, under the LOCAL workspace root) to the
/// corresponding REMOTE absolute path under `remote_workspace`. Errors if `cwd`
/// is not under `local_workspace_root`. Mirrors Docker's cwd→mount mapping.
pub(super) fn map_cwd_to_remote(
    local_workspace_root: &Path,
    remote_workspace: &Path,
    cwd: &Path,
) -> Result<String, HarnessError> {
    let relative =
        cwd.strip_prefix(local_workspace_root)
            .map_err(|_| HarnessError::ToolFailed {
                name: TOOL_NAME.to_string(),
                message: format!(
                    "command working directory {} is not under the SSH local workspace root {}",
                    cwd.display(),
                    local_workspace_root.display()
                ),
            })?;
    let ws = workspace_str(remote_workspace);
    if relative.as_os_str().is_empty() {
        Ok(ws)
    } else {
        let rel = relative.to_string_lossy().replace('\\', "/");
        Ok(format!("{ws}/{rel}"))
    }
}

/// Re-root a workspace-relative `rel` at the remote workspace, after the lexical
/// `..`/empty/absolute checks (the remote-side symlink boundary is enforced in
/// each fs command). Absolute paths that resolve inside are allowed only for
/// reads (writes reject absolute separately, like local).
pub(super) fn remote_workspace_path(
    remote_workspace: &Path,
    rel: &Path,
) -> Result<String, HarnessError> {
    reject_unsafe_components(rel)?;
    let ws = workspace_str(remote_workspace);
    if rel.is_absolute() {
        // An absolute path is passed through as-is; the per-command readlink
        // boundary check then rejects it if it lands outside the workspace
        // (mirroring local's "absolute allowed only if it canonicalizes inside").
        Ok(rel.to_string_lossy().replace('\\', "/"))
    } else {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.is_empty() || rel_str == "." {
            Ok(ws)
        } else {
            Ok(format!("{ws}/{rel_str}"))
        }
    }
}

/// Reject `..` and empty path components — the lexical pre-check mirrored from
/// [`local_fs`](super::super::local_fs) so the backend is self-contained.
pub(super) fn reject_unsafe_components(path: &Path) -> Result<(), HarnessError> {
    use std::ffi::OsStr;
    use std::path::Component;
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(HarnessError::WorkspaceEscape {
                    path: path.display().to_string(),
                });
            }
            Component::Normal(part) if part == OsStr::new("") => {
                return Err(HarnessError::InvalidWorkspacePath {
                    path: path.display().to_string(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reject an absolute path for write/create ops (matches local `resolve_for_create`).
pub(super) fn reject_absolute_for_write(rel: &Path) -> Result<(), HarnessError> {
    reject_unsafe_components(rel)?;
    if rel.is_absolute() {
        return Err(HarnessError::WorkspaceEscape {
            path: rel.display().to_string(),
        });
    }
    Ok(())
}

// === Parsers (unit-tested) ===================================================

/// Parse `stat -c '%F|%s'` output into a [`FileKind`]. Empty output means the
/// path did not exist (the command exits 0 with no output for that case).
pub(super) fn parse_metadata(stdout: &str) -> FileKind {
    let line = stdout.trim();
    if line.is_empty() {
        return FileKind::MISSING;
    }
    let (kind, size) = match line.split_once('|') {
        Some((k, s)) => (k.trim(), s.trim().parse::<u64>().unwrap_or(0)),
        None => (line, 0),
    };
    // GNU `stat -c %F` prints e.g. "regular file", "regular empty file",
    // "directory", "symbolic link".
    let is_dir = kind == "directory";
    let is_file = kind.contains("regular");
    FileKind {
        exists: true,
        is_file,
        is_dir,
        size: if is_file { size } else { 0 },
    }
}

/// Parse `find -printf '%y %f\n'` output into directory entries. Each line is
/// `<type-letter> <name>`; `d` is a directory, anything else (including symlinks)
/// is treated as a non-directory entry.
pub(super) fn parse_read_dir(stdout: &str) -> Vec<DirEntry> {
    stdout
        .lines()
        .filter_map(|line| {
            let (ty, name) = line.split_once(' ')?;
            if name.is_empty() {
                return None;
            }
            Some(DirEntry {
                name: name.to_string(),
                is_dir: ty == "d",
            })
        })
        .collect()
}

/// Parse `find ... -type f` absolute-path output into workspace-root-relative,
/// `/`-normalized paths. Lines not under the workspace are skipped defensively.
pub(super) fn parse_walk<'a>(
    stdout: &'a str,
    workspace: &'a str,
) -> impl Iterator<Item = String> + 'a {
    let prefix = if workspace == "/" {
        "/".to_string()
    } else {
        format!("{workspace}/")
    };
    stdout.lines().filter_map(move |line| {
        let line = line.trim_end_matches(['\r']);
        if line.is_empty() {
            return None;
        }
        line.strip_prefix(&prefix).map(|rel| rel.to_string())
    })
}
