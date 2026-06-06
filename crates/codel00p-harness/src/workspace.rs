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
