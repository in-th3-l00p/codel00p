use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::{
    errors::HarnessError,
    terminal::{FileKind, LocalBackend, TerminalBackend},
};

/// The workspace security boundary, now a thin facade over a
/// [`TerminalBackend`].
///
/// `Workspace` performs the *lexical* pre-checks (reject `..` / empty / absolute
/// path components) and delegates actual filesystem I/O — plus the on-disk
/// `canonicalize` + `starts_with(root)` boundary enforcement — to the backend.
/// For the local and Docker backends this is workspace-bounded local fs (Docker
/// bind-mounts the host workspace), so behavior is identical to the pre-seam
/// implementation; a future remote backend can satisfy the same contract.
///
/// [`Workspace::new`] defaults the backend to a [`LocalBackend::rooted`] at the
/// workspace root, so existing call sites are unchanged. [`Workspace::with_backend`]
/// shares an explicit backend (e.g. the one the command tools use) so file and
/// command tools operate on the same backend.
#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
    backend: Arc<dyn TerminalBackend>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("root", &self.root)
            .finish()
    }
}

impl PartialEq for Workspace {
    /// Two workspaces are equal when they share a root. The backend is an opaque
    /// trait object and is intentionally excluded (mirrors the historical
    /// root-only identity used by tests).
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Eq for Workspace {}

impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, HarnessError> {
        let root = canonical_root(root.as_ref())?;
        let backend = Arc::new(LocalBackend::rooted(root.clone()));
        Ok(Self { root, backend })
    }

    /// Build a workspace whose filesystem I/O is routed through `backend`. The
    /// backend is responsible for rooting workspace-relative paths at, and
    /// keeping them within, `root` (the local and Docker backends both do this
    /// via the shared workspace-bounded local-fs impl).
    pub fn with_backend(
        root: impl AsRef<Path>,
        backend: Arc<dyn TerminalBackend>,
    ) -> Result<Self, HarnessError> {
        let root = canonical_root(root.as_ref())?;
        Ok(Self { root, backend })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, path: impl AsRef<Path>) -> Result<PathBuf, HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;

        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };

        let resolved = candidate.canonicalize()?;
        if !resolved.starts_with(&self.root) {
            return Err(HarnessError::WorkspaceEscape {
                path: path.display().to_string(),
            });
        }

        Ok(resolved)
    }

    pub fn resolve_for_create(&self, path: impl AsRef<Path>) -> Result<PathBuf, HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        if path.is_absolute() {
            return Err(HarnessError::WorkspaceEscape {
                path: path.display().to_string(),
            });
        }

        let candidate = self.root.join(path);
        let parent = candidate
            .parent()
            .ok_or_else(|| HarnessError::InvalidWorkspacePath {
                path: path.display().to_string(),
            })?;
        let resolved_parent = if parent.exists() {
            parent.canonicalize()?
        } else {
            let existing_parent = nearest_existing_parent(parent)?;
            existing_parent.canonicalize()?
        };

        if !resolved_parent.starts_with(&self.root) {
            return Err(HarnessError::WorkspaceEscape {
                path: path.display().to_string(),
            });
        }

        Ok(candidate)
    }

    pub fn resolve_existing_file(&self, path: impl AsRef<Path>) -> Result<PathBuf, HarnessError> {
        let path = path.as_ref();
        let resolved = self.resolve(path)?;
        if !resolved.is_file() {
            return Err(HarnessError::ToolFailed {
                name: "workspace".to_string(),
                message: format!("file does not exist: {}", path.display()),
            });
        }
        Ok(resolved)
    }

    pub fn resolve_directory(&self, path: impl AsRef<Path>) -> Result<PathBuf, HarnessError> {
        let path = path.as_ref();
        let resolved = self.resolve(path)?;
        if !resolved.is_dir() {
            return Err(HarnessError::InvalidWorkspacePath {
                path: path.display().to_string(),
            });
        }
        Ok(resolved)
    }

    // --- Backend-delegated I/O facade ------------------------------------
    //
    // These do the lexical pre-checks here and delegate the actual I/O (and the
    // on-disk canonicalize/`starts_with(root)` boundary enforcement) to the
    // backend. Tools call these instead of touching `std::fs` so all workspace
    // I/O flows through the configured execution backend.

    /// Read a workspace file's raw bytes.
    pub fn read_bytes(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        self.backend.read_file(path)
    }

    /// Read a workspace file as UTF-8 text (errors on invalid UTF-8, like
    /// `fs::read_to_string`).
    pub fn read_utf8(&self, path: impl AsRef<Path>) -> Result<String, HarnessError> {
        let bytes = self.read_bytes(path)?;
        String::from_utf8(bytes).map_err(|error| {
            HarnessError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.utf8_error(),
            ))
        })
    }

    /// Write `contents` to a workspace path, creating parent directories as
    /// needed (matching `create_file`'s historical behavior).
    pub fn write(
        &self,
        path: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> Result<(), HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        if path.is_absolute() {
            return Err(HarnessError::WorkspaceEscape {
                path: path.display().to_string(),
            });
        }
        self.backend.write_file(path, contents.as_ref())
    }

    /// Delete a workspace file.
    pub fn delete(&self, path: impl AsRef<Path>) -> Result<(), HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        self.backend.delete_file(path)
    }

    /// Recursively create a workspace directory.
    pub fn create_dir_all(&self, path: impl AsRef<Path>) -> Result<(), HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        if path.is_absolute() {
            return Err(HarnessError::WorkspaceEscape {
                path: path.display().to_string(),
            });
        }
        self.backend.create_dir_all(path)
    }

    /// Whether a workspace path exists.
    pub fn exists(&self, path: impl AsRef<Path>) -> Result<bool, HarnessError> {
        Ok(self.metadata(path)?.exists)
    }

    /// Whether a workspace path is an existing regular file.
    pub fn is_file(&self, path: impl AsRef<Path>) -> Result<bool, HarnessError> {
        Ok(self.metadata(path)?.is_file)
    }

    /// Whether a workspace path is an existing directory.
    pub fn is_dir(&self, path: impl AsRef<Path>) -> Result<bool, HarnessError> {
        Ok(self.metadata(path)?.is_dir)
    }

    /// Existence / kind of a workspace path (does not error on a missing path).
    pub fn metadata(&self, path: impl AsRef<Path>) -> Result<FileKind, HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        self.backend.metadata(path)
    }

    /// Recursively walk a workspace directory (or file) `path`, invoking `visit`
    /// with each file's path relative to the workspace root (`/`-normalized).
    ///
    /// Does the lexical pre-checks here (reject `..` / empty components) and
    /// delegates the traversal — and its on-disk boundary enforcement — to the
    /// backend, so traversal flows through the configured execution backend just
    /// like the single-path I/O does. Skips the default build/VCS directories
    /// unless `include_ignored`, is resilient to unreadable nested directories,
    /// and hard-errors only if `path` itself is unreadable (the backend's `walk`
    /// contract).
    pub fn walk(
        &self,
        path: impl AsRef<Path>,
        include_ignored: bool,
        visit: &mut dyn FnMut(&str),
    ) -> Result<(), HarnessError> {
        let path = path.as_ref();
        reject_unsafe_components(path)?;
        self.backend.walk(path, include_ignored, visit)
    }
}

/// Canonicalize and validate a workspace root: it must exist and be a directory.
fn canonical_root(root: &Path) -> Result<PathBuf, HarnessError> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(HarnessError::InvalidWorkspacePath {
            path: root.display().to_string(),
        });
    }
    Ok(root)
}

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
