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

use crate::errors::HarnessError;

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
}

impl FileKind {
    /// A `FileKind` for a path that does not exist.
    pub const MISSING: FileKind = FileKind {
        exists: false,
        is_file: false,
        is_dir: false,
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
