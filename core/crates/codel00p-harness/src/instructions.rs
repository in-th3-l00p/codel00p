use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{errors::HarnessError, workspace::Workspace};

const INSTRUCTION_FILES: [&str; 3] = ["CODEL00P.md", "AGENTS.md", "CLAUDE.md"];

/// Maximum number of ancestor directories to walk above the workspace root when
/// collecting instruction files. Bounds work in deep trees; the walk also stops
/// early at the filesystem root or the user's home directory (see [`load`]).
const ANCESTOR_DEPTH_CAP: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInstruction {
    source: String,
    content: String,
}

impl ProjectInstruction {
    pub fn new(source: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            content: normalize_instruction_content(content.into()),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInstructions {
    instructions: Vec<ProjectInstruction>,
}

impl ProjectInstructions {
    pub fn new(instructions: Vec<ProjectInstruction>) -> Self {
        Self { instructions }
    }

    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    pub fn items(&self) -> &[ProjectInstruction] {
        &self.instructions
    }

    pub fn sources(&self) -> Vec<&str> {
        self.instructions
            .iter()
            .map(ProjectInstruction::source)
            .collect()
    }

    pub fn as_prompt(&self) -> String {
        let mut prompt = String::from("Project instructions:");
        for instruction in &self.instructions {
            prompt.push_str("\n## ");
            prompt.push_str(instruction.source());
            prompt.push('\n');
            prompt.push_str(instruction.content());
            prompt.push('\n');
        }
        prompt.pop();
        prompt
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectInstructionLoader;

impl ProjectInstructionLoader {
    /// Load instruction files for the workspace.
    ///
    /// Files named in [`INSTRUCTION_FILES`] are collected from the workspace
    /// root and from each ancestor directory above it, so monorepo subprojects
    /// pick up surrounding context (Claude Code and Hermes both walk ancestors
    /// this way).
    ///
    /// Ordering / precedence (most specific first): the workspace-root files
    /// always come first and keep their historical order
    /// (`CODEL00P.md`, `AGENTS.md`, `CLAUDE.md`). Ancestor files are appended as
    /// additional, lower-priority context, ordered nearest-ancestor first. The
    /// resulting [`ProjectInstructions::as_prompt`] therefore lists the most
    /// specific (root) instructions first and the most distant ancestor last.
    /// Root-only projects are unaffected: a normal repo has no instruction files
    /// in its parent directories, so nothing extra is added.
    ///
    /// The ancestor walk stops at the filesystem root, at the user's home
    /// directory, or after [`ANCESTOR_DEPTH_CAP`] parents, whichever comes
    /// first. The same file is never read twice, empty files are skipped, and
    /// unreadable directories are skipped gracefully.
    pub fn load(&self, workspace: &Workspace) -> Result<ProjectInstructions, HarnessError> {
        let root = workspace.root();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut instructions = Vec::new();

        // Workspace-root files: highest priority, historical order. Read errors
        // here surface (callers expect the configured root to be readable).
        collect_dir(root, &mut seen, &mut instructions, /* strict = */ true)?;

        // Ancestor directories: additive, lower-priority context. Degrade
        // gracefully on unreadable dirs/files rather than failing the load.
        let home = home_dir();
        let mut current = root.to_path_buf();
        for _ in 0..ANCESTOR_DEPTH_CAP {
            // Stop once we hit the user's home directory (do not climb above it).
            if home.as_deref() == Some(current.as_path()) {
                break;
            }
            let Some(parent) = current.parent().map(Path::to_path_buf) else {
                break; // filesystem root
            };
            if parent == current {
                break; // defensive: no progress
            }
            collect_dir(
                &parent,
                &mut seen,
                &mut instructions,
                /* strict = */ false,
            )?;
            if home.as_deref() == Some(parent.as_path()) {
                break;
            }
            current = parent;
        }

        Ok(ProjectInstructions::new(instructions))
    }
}

/// Collect instruction files from a single directory in [`INSTRUCTION_FILES`]
/// order, skipping non-files, empty files, and already-seen paths. When
/// `strict` is false, unreadable files are silently skipped instead of erroring.
fn collect_dir(
    dir: &Path,
    seen: &mut HashSet<PathBuf>,
    instructions: &mut Vec<ProjectInstruction>,
    strict: bool,
) -> Result<(), HarnessError> {
    for file_name in INSTRUCTION_FILES {
        let path = dir.join(file_name);
        if !path.is_file() {
            continue;
        }
        // Dedupe on canonical path so the same file reachable via two routes is
        // read once; fall back to the literal path if canonicalization fails.
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            continue;
        }
        let content = match read_instruction_file(&path) {
            Ok(content) => content,
            Err(error) if strict => return Err(error),
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }
        let source = if instructions
            .iter()
            .any(|existing| existing.source() == file_name)
        {
            // Disambiguate same-named ancestor files by their directory.
            format!("{} ({})", file_name, dir.display())
        } else {
            file_name.to_string()
        };
        instructions.push(ProjectInstruction::new(source, content));
    }
    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .and_then(|home| std::fs::canonicalize(&home).ok().or(Some(home)))
}

fn read_instruction_file(path: &Path) -> Result<String, HarnessError> {
    Ok(std::fs::read_to_string(path)?)
}

fn normalize_instruction_content(content: String) -> String {
    content.trim().replace("\r\n", "\n")
}
