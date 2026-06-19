//! Shared, workspace-bounded local filesystem implementation for backends.
//!
//! Both [`LocalBackend`](super::LocalBackend) and
//! [`DockerBackend`](super::DockerBackend) expose the workspace filesystem as
//! plain local I/O rooted at the workspace directory: `LocalBackend` runs
//! commands on the host, and `DockerBackend` bind-mounts the *host* workspace
//! into the container, so in both cases the bytes on disk live at the host
//! workspace root. Routing both backends' fs ops through this module keeps that
//! single source of truth and means there is exactly one place enforcing the
//! workspace security boundary.
//!
//! # Security boundary (must match `workspace.rs` exactly)
//!
//! The functions here re-root a *workspace-relative* path at the backend's
//! workspace root and then apply the same checks `Workspace` historically did
//! before delegating I/O:
//!
//! * **Reads / existing paths** ([`read_file`], [`delete_file`], [`metadata`],
//!   [`read_dir`]): canonicalize the candidate and require the real path to
//!   `starts_with(root)`. Because `canonicalize` resolves symlinks, a symlink
//!   inside the workspace that points outside it is rejected — the
//!   canonicalize-based escape protection.
//! * **Writes / creates** ([`write_file`], [`create_dir_all`]): reject absolute
//!   paths, then canonicalize the *nearest existing parent* and require it to
//!   `starts_with(root)`. This validates the destination before creating it,
//!   matching `Workspace::resolve_for_create`.
//!
//! `..` and empty path components are rejected up front in every case, mirroring
//! `reject_unsafe_components`. The lexical pre-checks (`..` / empty / absolute)
//! are *also* performed by [`Workspace`](crate::workspace::Workspace) before it
//! reaches a backend; they are re-checked here so the backend is self-contained
//! and safe even if called directly (and so a remote backend implementing the
//! same contract has the rules spelled out).

use std::{
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::{
    errors::HarnessError,
    walk::{DEFAULT_IGNORED_DIRS, MAX_FILES_WALKED, normalize_path},
};

/// What a path points at on the backend's filesystem.
///
/// Returned by [`TerminalBackend::metadata`](super::TerminalBackend::metadata);
/// enough for the existence / is-file / is-dir checks that
/// `resolve_existing_file` and `resolve_directory` need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileKind {
    /// Whether the path exists at all.
    pub exists: bool,
    /// Whether it is a regular file.
    pub is_file: bool,
    /// Whether it is a directory.
    pub is_dir: bool,
    /// Size in bytes for a regular file (0 for missing paths and directories).
    /// Lets callers cheaply skip oversized files without reading them.
    pub size: u64,
}

impl FileKind {
    /// A `FileKind` for a path that does not exist.
    pub const MISSING: FileKind = FileKind {
        exists: false,
        is_file: false,
        is_dir: false,
        size: 0,
    };
}

/// One entry of a directory listing from
/// [`TerminalBackend::read_dir`](super::TerminalBackend::read_dir).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// The entry's file name (the final component, not a path).
    pub name: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
}

/// Reject `..` and empty path components, exactly as
/// `workspace::reject_unsafe_components` does.
fn reject_unsafe_components(path: &Path) -> Result<(), HarnessError> {
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

/// The first ancestor of `path` that exists on disk, mirroring
/// `workspace::nearest_existing_parent`.
fn nearest_existing_parent(path: &Path) -> Result<&Path, HarnessError> {
    for ancestor in path.ancestors() {
        if ancestor.exists() {
            return Ok(ancestor);
        }
    }
    Err(HarnessError::InvalidWorkspacePath {
        path: path.display().to_string(),
    })
}

/// Resolve a relative path for *reading* an existing entry: re-root at `root`,
/// canonicalize, and require the real path to stay inside `root`. Equivalent to
/// `Workspace::resolve`, so symlink escapes are caught by `canonicalize`.
///
/// Accepts an absolute `rel` only when it already canonicalizes inside `root`
/// (matching the historical `Workspace::resolve`, which allowed absolute inputs
/// as long as the resolved path stayed within the workspace).
fn resolve_existing(root: &Path, rel: &Path) -> Result<PathBuf, HarnessError> {
    reject_unsafe_components(rel)?;

    let candidate = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        root.join(rel)
    };

    let resolved = candidate.canonicalize()?;
    if !resolved.starts_with(root) {
        return Err(HarnessError::WorkspaceEscape {
            path: rel.display().to_string(),
        });
    }
    Ok(resolved)
}

/// Resolve a relative path for *creating* an entry: reject absolute, re-root at
/// `root`, and require the nearest existing parent to canonicalize inside
/// `root`. Equivalent to `Workspace::resolve_for_create`. Returns the
/// (non-canonicalized) destination path to write to.
fn resolve_for_create(root: &Path, rel: &Path) -> Result<PathBuf, HarnessError> {
    reject_unsafe_components(rel)?;
    if rel.is_absolute() {
        return Err(HarnessError::WorkspaceEscape {
            path: rel.display().to_string(),
        });
    }

    let candidate = root.join(rel);
    let parent = candidate
        .parent()
        .ok_or_else(|| HarnessError::InvalidWorkspacePath {
            path: rel.display().to_string(),
        })?;
    let resolved_parent = if parent.exists() {
        parent.canonicalize()?
    } else {
        nearest_existing_parent(parent)?.canonicalize()?
    };

    if !resolved_parent.starts_with(root) {
        return Err(HarnessError::WorkspaceEscape {
            path: rel.display().to_string(),
        });
    }

    Ok(candidate)
}

/// Read a workspace-relative file's bytes, enforcing the read boundary.
pub fn read_file(root: &Path, rel: &Path) -> Result<Vec<u8>, HarnessError> {
    let resolved = resolve_existing(root, rel)?;
    Ok(fs::read(resolved)?)
}

/// Write `contents` to a workspace-relative path, creating parent directories as
/// needed, enforcing the create boundary (so the parent stays in the workspace).
pub fn write_file(root: &Path, rel: &Path, contents: &[u8]) -> Result<(), HarnessError> {
    let destination = resolve_for_create(root, rel)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, contents)?;
    Ok(())
}

/// Delete a workspace-relative file, enforcing the read boundary (the file must
/// already exist inside the workspace).
pub fn delete_file(root: &Path, rel: &Path) -> Result<(), HarnessError> {
    let resolved = resolve_existing(root, rel)?;
    fs::remove_file(resolved)?;
    Ok(())
}

/// Recursively create a workspace-relative directory, enforcing the create
/// boundary.
pub fn create_dir_all(root: &Path, rel: &Path) -> Result<(), HarnessError> {
    let destination = resolve_for_create(root, rel)?;
    fs::create_dir_all(&destination)?;
    Ok(())
}

/// Report whether a workspace-relative path exists and what it is.
///
/// A path that does not exist returns [`FileKind::MISSING`] rather than an
/// error, so callers can do existence checks. `..` / empty / absolute-escaping
/// paths still error (they are never valid workspace paths).
pub fn metadata(root: &Path, rel: &Path) -> Result<FileKind, HarnessError> {
    match resolve_existing(root, rel) {
        Ok(resolved) => {
            let meta = fs::metadata(&resolved)?;
            Ok(FileKind {
                exists: true,
                is_file: meta.is_file(),
                is_dir: meta.is_dir(),
                // Size is meaningful only for regular files; report 0 otherwise so
                // callers can stat-gate oversized files without reading them.
                size: if meta.is_file() { meta.len() } else { 0 },
            })
        }
        // A not-found canonicalize is a legitimate "does not exist" answer.
        Err(HarnessError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(FileKind::MISSING)
        }
        Err(error) => Err(error),
    }
}

/// List the immediate entries of a workspace-relative directory (name + is_dir),
/// enforcing the read boundary on the directory itself.
pub fn read_dir(root: &Path, rel: &Path) -> Result<Vec<DirEntry>, HarnessError> {
    let resolved = resolve_existing(root, rel)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(resolved)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type()?.is_dir();
        entries.push(DirEntry { name, is_dir });
    }
    Ok(entries)
}

// --- Directory traversal (initiative #7, phase 3 — PR2) -----------------------
//
// The shared, efficient local-fs walk used by both `LocalBackend::walk` and
// `DockerBackend::walk` (Docker bind-mounts the host workspace, so the bytes
// live at the host root). This is the historical `walk::walk_files` body moved
// verbatim — it operates directly on absolute host paths under `root`, with no
// per-directory `canonicalize`, so local traversal keeps identical behavior AND
// performance (including the current symlink-following). A future remote backend
// instead uses the default `read_dir`-based `TerminalBackend::walk`.

/// Walk a *workspace-relative* directory (or file) `rel`: resolve it inside
/// `root` (the same read boundary as [`read_file`] / [`read_dir`], so symlink
/// escapes are rejected), then delegate to [`walk`]. This is what
/// [`LocalBackend::walk`](super::LocalBackend) and
/// [`DockerBackend::walk`](super::DockerBackend) call, so both share one
/// resolution + traversal path that emits workspace-root-relative `/`-normalized
/// file paths.
pub fn walk_rel(
    root: &Path,
    rel: &Path,
    include_ignored: bool,
    visit: &mut dyn FnMut(&str),
) -> Result<(), HarnessError> {
    let resolved = resolve_existing(root, rel)?;
    walk(root, &resolved, include_ignored, visit)
}

/// Recursively visit every file under `current` (an absolute path under `root`),
/// invoking `visit` with each file's path relative to `root` (normalized with
/// `/` separators). Skips the default build/VCS directories unless
/// `include_ignored`, and bails out after [`MAX_FILES_WALKED`] files as a
/// runaway guard.
///
/// Resilient to read errors in nested subdirectories: an unreadable nested
/// directory or unreadable entry is skipped and the walk continues. Only a
/// failure to read the top-level `current` path is surfaced as an error, since
/// that is the caller's explicitly requested root.
pub fn walk(
    root: &Path,
    current: &Path,
    include_ignored: bool,
    visit: &mut dyn FnMut(&str),
) -> Result<(), HarnessError> {
    let mut state = WalkState {
        visited: 0,
        skipped_unreadable: 0,
    };

    if current.is_file() {
        if let Ok(relative) = current.strip_prefix(root) {
            visit(&normalize_path(relative));
        }
        return Ok(());
    }

    // The root directory must be readable: a failure here is a legitimate hard
    // error (the caller asked for exactly this path). Failures deeper in the
    // tree are tolerated by `walk_dir_resilient`.
    let entries = fs::read_dir(current)?;
    walk_entries(root, entries, include_ignored, &mut state, visit);
    Ok(())
}

/// Mutable bookkeeping threaded through the recursive walk.
struct WalkState {
    visited: usize,
    /// Count of nested directories/entries skipped because they could not be
    /// read. Tracked so callers *could* surface it later without a breaking
    /// change to the `visit` signature; not currently exposed.
    #[allow(dead_code)]
    skipped_unreadable: usize,
}

/// Recursively walk a nested directory, tolerating read errors by skipping the
/// offending directory or entry and continuing.
fn walk_dir_resilient(
    root: &Path,
    current: &Path,
    include_ignored: bool,
    state: &mut WalkState,
    visit: &mut dyn FnMut(&str),
) {
    match fs::read_dir(current) {
        Ok(entries) => walk_entries(root, entries, include_ignored, state, visit),
        Err(_) => {
            // Unreadable nested directory (e.g. EACCES/EPERM). Skip it and
            // continue the walk instead of aborting the whole listing.
            state.skipped_unreadable += 1;
        }
    }
}

/// Drain a `ReadDir` iterator, recursing into subdirectories and visiting files.
/// Individual entry errors are tolerated and skipped.
fn walk_entries(
    root: &Path,
    entries: fs::ReadDir,
    include_ignored: bool,
    state: &mut WalkState,
    visit: &mut dyn FnMut(&str),
) {
    for entry in entries {
        if state.visited >= MAX_FILES_WALKED {
            return;
        }
        let Ok(entry) = entry else {
            // The entry's metadata could not be read; skip it and continue.
            state.skipped_unreadable += 1;
            continue;
        };
        let path = entry.path();

        if path.is_dir() {
            if !include_ignored
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| DEFAULT_IGNORED_DIRS.contains(&name))
            {
                continue;
            }
            walk_dir_resilient(root, &path, include_ignored, state, visit);
        } else if path.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            visit(&normalize_path(relative));
            state.visited += 1;
        }
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;

    /// A nested unreadable directory must be skipped, not abort the walk.
    #[cfg(unix)]
    #[test]
    fn walk_skips_unreadable_nested_dir() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("top.txt"), "x").unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("inner.txt"), "y").unwrap();

        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let _restore = Restore(locked.clone());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let mut seen = Vec::new();
        walk(dir.path(), dir.path(), false, &mut |r| {
            seen.push(r.to_string())
        })
        .expect("walk must tolerate an unreadable nested dir");

        assert!(seen.iter().any(|s| s == "top.txt"), "got {seen:?}");
    }

    /// An unreadable ROOT path is a legitimate hard error and must surface.
    #[cfg(unix)]
    #[test]
    fn walk_errors_on_unreadable_root() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir(&root).unwrap();

        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let _restore = Restore(root.clone());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();

        let result = walk(&root, &root, false, &mut |_| {});
        assert!(
            result.is_err(),
            "an unreadable root must surface as a hard error"
        );
    }
}
