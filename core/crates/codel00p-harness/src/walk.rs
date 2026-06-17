//! Shared workspace-walking helpers used by the file-navigation tools
//! (`find_files`, `grep`) and the repo map. Walks skip the usual
//! build/dependency/VCS sinks by default so tools are not flooded with
//! generated or vendored files.

use std::{fs, path::Path};

use regex::Regex;

use crate::errors::HarnessError;

/// Directory names skipped by default while walking. These are the usual
/// build/dependency/VCS sinks that bloat results without adding signal.
pub(crate) const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".gradle",
    ".idea",
    ".vscode",
    "vendor",
];

/// Hard ceiling on files visited during a single walk, so a pathological tree
/// cannot hang a tool call.
pub(crate) const MAX_FILES_WALKED: usize = 100_000;

/// Recursively visit every file under `current`, invoking `visit` with each
/// file's path relative to `root` (normalized with `/` separators). Skips the
/// default build/VCS directories unless `include_ignored` is set, and bails out
/// after [`MAX_FILES_WALKED`] files as a runaway guard.
///
/// The walk is resilient to read errors in nested subdirectories: if a
/// directory cannot be listed (e.g. an `EACCES`/`EPERM` permission error from a
/// macOS TCC-protected directory) or an individual entry's metadata cannot be
/// read, that directory/entry is skipped and the walk continues rather than
/// aborting. Only a failure to read the top-level `current` path is surfaced as
/// an error, since that is the caller's explicitly requested root.
pub(crate) fn walk_files(
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

pub(crate) fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A compiled glob. Matches against the file name only when the source pattern
/// contains no `/`, otherwise against the full relative path.
pub(crate) struct GlobMatcher {
    regex: Regex,
    /// True when the pattern has no path separator and should match the file
    /// name rather than the full relative path.
    basename_only: bool,
}

impl GlobMatcher {
    pub(crate) fn compile(tool: &str, pattern: &str) -> Result<Self, HarnessError> {
        let basename_only = !pattern.contains('/');
        let regex = Regex::new(&glob_to_regex(pattern)).map_err(|error| {
            HarnessError::InvalidToolInput {
                name: tool.to_string(),
                message: format!("invalid glob `{pattern}`: {error}"),
            }
        })?;
        Ok(Self {
            regex,
            basename_only,
        })
    }

    pub(crate) fn is_match(&self, relative_path: &str) -> bool {
        let candidate = if self.basename_only {
            relative_path.rsplit('/').next().unwrap_or(relative_path)
        } else {
            relative_path
        };
        self.regex.is_match(candidate)
    }
}

/// Translate a glob pattern into an anchored regular expression.
///
/// `**` matches any run of characters (including `/`), `*` matches any run that
/// does not cross a `/`, and `?` matches a single non-`/` character. Every other
/// character is matched literally (regex metacharacters are escaped).
pub(crate) fn glob_to_regex(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() + 8);
    out.push('^');
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    out.push_str(".*");
                    i += 2;
                    // Swallow a `/` immediately after `**` so `**/foo` also
                    // matches `foo` at the root.
                    if i < chars.len() && chars[i] == '/' {
                        out.push_str("/?");
                        i += 1;
                    }
                    continue;
                }
                out.push_str("[^/]*");
            }
            '?' => out.push_str("[^/]"),
            c => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_to_regex_handles_stars_and_question() {
        assert_eq!(glob_to_regex("*.rs"), "^[^/]*\\.rs$");
        assert_eq!(glob_to_regex("src/**/*.rs"), "^src/.*/?[^/]*\\.rs$");
        assert_eq!(glob_to_regex("a?c"), "^a[^/]c$");
    }

    #[test]
    fn glob_double_star_matches_root_and_nested() {
        let matcher = GlobMatcher::compile("find_files", "src/**/mod.rs").unwrap();
        assert!(matcher.is_match("src/mod.rs"));
        assert!(matcher.is_match("src/a/b/mod.rs"));
        assert!(!matcher.is_match("other/mod.rs"));
    }

    #[test]
    fn glob_basename_matches_anywhere() {
        let matcher = GlobMatcher::compile("find_files", "*.rs").unwrap();
        assert!(matcher.is_match("deep/nested/lib.rs"));
        assert!(!matcher.is_match("deep/nested/lib.py"));
    }

    /// A nested unreadable directory must be skipped, not abort the walk.
    #[cfg(unix)]
    #[test]
    fn walk_files_skips_unreadable_nested_dir() {
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
        walk_files(dir.path(), dir.path(), false, &mut |r| {
            seen.push(r.to_string())
        })
        .expect("walk must tolerate an unreadable nested dir");

        assert!(seen.iter().any(|s| s == "top.txt"), "got {seen:?}");
    }

    /// An unreadable ROOT path is a legitimate hard error and must surface.
    #[cfg(unix)]
    #[test]
    fn walk_files_errors_on_unreadable_root() {
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

        let result = walk_files(&root, &root, false, &mut |_| {});
        assert!(
            result.is_err(),
            "an unreadable root must surface as a hard error"
        );
    }
}
