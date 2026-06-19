//! Shared workspace-walking helpers used by the file-navigation tools
//! (`find_files`, `grep`) and the repo map.
//!
//! The traversal itself now lives behind the execution backend: directory walks
//! go through [`Workspace::walk`](crate::workspace::Workspace::walk), which
//! delegates to the configured [`TerminalBackend`](crate::terminal::TerminalBackend)
//! so a remote backend can walk a remote workspace. The local/Docker backends
//! use the efficient absolute-path walk in
//! [`local_fs::walk`](crate::terminal::local_fs::walk). This module keeps the
//! traversal *policy* shared between those code paths — the default ignored-dir
//! set, the file-count ceiling, and `/`-normalization — plus the glob matcher.

use std::path::Path;

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
}
