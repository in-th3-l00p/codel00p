use std::{
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::errors::HarnessError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, HarnessError> {
        let root = root.as_ref().canonicalize()?;

        if !root.is_dir() {
            return Err(HarnessError::InvalidWorkspacePath {
                path: root.display().to_string(),
            });
        }

        Ok(Self { root })
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

    pub fn read_utf8(&self, path: impl AsRef<Path>) -> Result<String, HarnessError> {
        let resolved = self.resolve(path)?;
        Ok(fs::read_to_string(resolved)?)
    }
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
