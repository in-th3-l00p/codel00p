//! Tool-failure classification for in-turn error self-correction (#12 T0.4).
//!
//! When a tool call fails during a turn — either the tool's `execute` returned
//! an `Err`, or it produced a result whose `error`/non-zero exit indicates
//! failure — the harness classifies the failure message into a small set of
//! actionable categories and attaches a short hint to the error payload fed back
//! to the model. The richer `{ "error", "error_kind", "hint" }` shape lets the
//! model recover deliberately (install a missing dependency, try a different
//! approach when permission-denied, etc.) instead of blindly retrying.
//!
//! This is a **dedicated** enum, intentionally separate from
//! [`codel00p_protocol::RuntimeErrorKind`]. That type models the *provider/route*
//! layer (auth, rate limit, context overflow, payload-too-large, billing) for
//! fallback routing and credential rotation — concerns that never apply to a
//! shell command or a file edit failing. [`ToolErrorKind`] models the *tool
//! execution* layer (missing dependency, permission, not-found, compile error,
//! …) whose recovery lever is a model-facing hint, not a route change. Keeping
//! them apart avoids overloading one enum with two unrelated vocabularies.
//!
//! The classifier is a pure function over the failure text so it is unit-testable
//! in isolation; matching is ordered (most specific signatures first) and
//! case-insensitive.

use std::fmt;

/// A category of tool-call failure, derived by signature-matching the failure
/// message/stderr. Each kind carries a stable snake_case wire name (surfaced as
/// `error_kind` in the tool result) and a short actionable hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolErrorKind {
    /// A required command, binary, library, or module is not installed.
    MissingDependency,
    /// The operation was blocked by filesystem/OS permissions or a policy.
    PermissionDenied,
    /// A referenced file, path, directory, or resource does not exist.
    NotFound,
    /// The operation exceeded a time limit.
    Timeout,
    /// Source failed to compile / parse (compiler or syntax error).
    CompileError,
    /// A network operation failed (connection refused, DNS, unreachable host).
    Network,
    /// The tool input was malformed or rejected as invalid.
    InvalidInput,
    /// Nothing matched — the failure could not be categorized.
    Unknown,
}

impl ToolErrorKind {
    /// The stable snake_case identifier surfaced as `error_kind` in the payload.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolErrorKind::MissingDependency => "missing_dependency",
            ToolErrorKind::PermissionDenied => "permission_denied",
            ToolErrorKind::NotFound => "not_found",
            ToolErrorKind::Timeout => "timeout",
            ToolErrorKind::CompileError => "compile_error",
            ToolErrorKind::Network => "network",
            ToolErrorKind::InvalidInput => "invalid_input",
            ToolErrorKind::Unknown => "unknown",
        }
    }

    /// A short, actionable hint for the model, or `None` for `Unknown` (nothing
    /// specific to add beyond the raw error).
    pub fn hint(self) -> Option<&'static str> {
        match self {
            ToolErrorKind::MissingDependency => Some(
                "the command failed because a required dependency or tool is missing; \
                 install it (or use an already-available alternative) rather than retrying as-is",
            ),
            ToolErrorKind::PermissionDenied => Some(
                "this was blocked by permissions; retrying the same way will not help — \
                 try a different approach, a path you can access, or ask the user",
            ),
            ToolErrorKind::NotFound => Some(
                "the target file, path, or resource does not exist; verify the location \
                 (list the directory or search) before retrying",
            ),
            ToolErrorKind::Timeout => Some(
                "the operation timed out; consider a smaller/faster command, a longer timeout, \
                 or splitting the work rather than repeating the same call",
            ),
            ToolErrorKind::CompileError => Some(
                "the code failed to compile or parse; read the reported error/location and \
                 fix the source before re-running",
            ),
            ToolErrorKind::Network => Some(
                "a network operation failed; the host may be unreachable or offline — \
                 avoid repeating it blindly and consider an offline alternative or asking the user",
            ),
            ToolErrorKind::InvalidInput => Some(
                "the tool rejected the input as invalid; re-check the arguments against the \
                 tool's expected schema before retrying",
            ),
            ToolErrorKind::Unknown => None,
        }
    }
}

impl fmt::Display for ToolErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classify a tool-failure message/stderr into a [`ToolErrorKind`] plus an
/// optional hint. Pure and deterministic: matching is case-insensitive and
/// ordered so the most specific signatures win.
pub fn classify(message: &str) -> (ToolErrorKind, Option<&'static str>) {
    let kind = classify_kind(message);
    (kind, kind.hint())
}

fn classify_kind(message: &str) -> ToolErrorKind {
    let lower = message.to_ascii_lowercase();

    // Permission first: "permission denied" can co-occur with "not found"-ish
    // wording, and the recovery (different approach / ask) is distinct.
    if contains_any(
        &lower,
        &[
            "permission denied",
            "operation not permitted",
            "access is denied",
            "eacces",
            "eperm",
            "must be run as root",
            "are you root",
            "sudo",
            "read-only file system",
            "denied by permission policy",
            "blocked by permission",
        ],
    ) {
        return ToolErrorKind::PermissionDenied;
    }

    // Missing dependency: a binary/module/library/package is not installed.
    if contains_any(
        &lower,
        &[
            "command not found",
            ": not found",
            "is not recognized as an internal or external command",
            "no such command",
            "executable not found",
            "modulenotfounderror",
            "no module named",
            "cannot find module",
            "library not loaded",
            "shared object",
            "unable to locate package",
            "could not find a version that satisfies",
            "is not installed",
            "command 'cargo' not found",
        ],
    ) {
        return ToolErrorKind::MissingDependency;
    }

    // Compile / syntax errors (before generic "error[" / not-found heuristics).
    if contains_any(
        &lower,
        &[
            "syntaxerror",
            "syntax error",
            "parse error",
            "error[e",
            "compilation failed",
            "could not compile",
            "expected one of",
            "unexpected token",
            "cannot find type",
            "mismatched types",
            "unterminated",
            "indentationerror",
        ],
    ) {
        return ToolErrorKind::CompileError;
    }

    // Timeout.
    if contains_any(
        &lower,
        &[
            "timed out",
            "timeout",
            "deadline exceeded",
            "etimedout",
            "operation timed out",
        ],
    ) {
        return ToolErrorKind::Timeout;
    }

    // Network.
    if contains_any(
        &lower,
        &[
            "connection refused",
            "connection reset",
            "could not resolve host",
            "name or service not known",
            "temporary failure in name resolution",
            "network is unreachable",
            "no route to host",
            "econnrefused",
            "enotfound",
            "dns",
            "tls handshake",
            "ssl error",
            "failed to connect",
        ],
    ) {
        return ToolErrorKind::Network;
    }

    // Invalid input / malformed arguments.
    if contains_any(
        &lower,
        &[
            "invalid input",
            "invalid argument",
            "invalid value",
            "invalid json",
            "failed to deserialize",
            "missing required",
            "unexpected argument",
            "unknown flag",
            "unrecognized option",
            "malformed",
        ],
    ) {
        return ToolErrorKind::InvalidInput;
    }

    // Not found (file/path/resource). Checked late so more specific kinds win.
    if contains_any(
        &lower,
        &[
            "no such file or directory",
            "not found",
            "does not exist",
            "enoent",
            "cannot access",
            "404",
            "no such directory",
        ],
    ) {
        return ToolErrorKind::NotFound;
    }

    ToolErrorKind::Unknown
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(message: &str) -> ToolErrorKind {
        classify(message).0
    }

    #[test]
    fn classifies_missing_dependency() {
        assert_eq!(
            kind("bash: cargo: command not found"),
            ToolErrorKind::MissingDependency
        );
        assert_eq!(
            kind("ModuleNotFoundError: No module named 'requests'"),
            ToolErrorKind::MissingDependency
        );
        assert_eq!(
            kind("E: Unable to locate package frobnicate"),
            ToolErrorKind::MissingDependency
        );
    }

    #[test]
    fn classifies_permission_denied() {
        assert_eq!(
            kind("touch: /etc/hosts: Permission denied"),
            ToolErrorKind::PermissionDenied
        );
        assert_eq!(
            kind("mkdir: cannot create directory: Operation not permitted"),
            ToolErrorKind::PermissionDenied
        );
        assert_eq!(
            kind("tool execution denied by permission policy"),
            ToolErrorKind::PermissionDenied
        );
    }

    #[test]
    fn classifies_not_found() {
        assert_eq!(
            kind("cat: foo.txt: No such file or directory"),
            ToolErrorKind::NotFound
        );
        assert_eq!(
            kind("the path /tmp/missing does not exist"),
            ToolErrorKind::NotFound
        );
    }

    #[test]
    fn classifies_timeout() {
        assert_eq!(
            kind("error: operation timed out after 30s"),
            ToolErrorKind::Timeout
        );
        assert_eq!(kind("context deadline exceeded"), ToolErrorKind::Timeout);
    }

    #[test]
    fn classifies_compile_error() {
        assert_eq!(
            kind("error[E0425]: cannot find value `x` in this scope"),
            ToolErrorKind::CompileError
        );
        assert_eq!(
            kind("  File \"x.py\", line 2\n    def\n       ^\nSyntaxError: invalid syntax"),
            ToolErrorKind::CompileError
        );
        assert_eq!(
            kind("error: could not compile `mycrate`"),
            ToolErrorKind::CompileError
        );
    }

    #[test]
    fn classifies_network() {
        assert_eq!(
            kind("curl: (7) Failed to connect to example.com port 443: Connection refused"),
            ToolErrorKind::Network
        );
        assert_eq!(
            kind("fatal: unable to access 'https://x/': Could not resolve host: x"),
            ToolErrorKind::Network
        );
    }

    #[test]
    fn classifies_invalid_input() {
        assert_eq!(
            kind("error: invalid value 'q' for '--mode'"),
            ToolErrorKind::InvalidInput
        );
        assert_eq!(
            kind("failed to deserialize JSON body"),
            ToolErrorKind::InvalidInput
        );
    }

    #[test]
    fn classifies_unknown() {
        assert_eq!(
            kind("something inexplicable happened"),
            ToolErrorKind::Unknown
        );
        assert_eq!(classify("boom").1, None);
    }

    #[test]
    fn known_kinds_carry_a_hint() {
        for k in [
            ToolErrorKind::MissingDependency,
            ToolErrorKind::PermissionDenied,
            ToolErrorKind::NotFound,
            ToolErrorKind::Timeout,
            ToolErrorKind::CompileError,
            ToolErrorKind::Network,
            ToolErrorKind::InvalidInput,
        ] {
            assert!(k.hint().is_some(), "{k} should carry a hint");
        }
        assert!(ToolErrorKind::Unknown.hint().is_none());
    }

    #[test]
    fn permission_wins_over_not_found() {
        // A message that mentions both should classify as permission (more
        // actionable recovery).
        assert_eq!(
            kind("open /root/secret: permission denied (file may not exist)"),
            ToolErrorKind::PermissionDenied
        );
    }
}
