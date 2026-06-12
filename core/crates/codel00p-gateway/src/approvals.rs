//! Pending-approval store for the chat `/approve` / `/deny` flow.
//!
//! When an agent turn needs a permission a remote chat user must grant, the
//! gateway records a *pending* approval keyed by conversation, replies asking
//! the user, and a later `/approve` or `/deny` resolves it.
//!
//! This module owns only the persistent **state** and the **decision logic**:
//! recording a pending request, reading it back, and consuming it with a
//! decision. Pausing the harness turn while a request is outstanding (wiring a
//! `PermissionPolicy` to consult this store) is the integrator's job.
//!
//! State is file-backed so it survives process restarts and is shared across
//! the (possibly many) processes an adapter may run: one
//! `<conversation>.json` file per pending request lives under a directory. The
//! conversation id is sanitized into the filename (alphanumeric, `-` and `_`
//! only) so a hostile id can never escape that directory.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A permission request awaiting a remote chat user's decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingApproval {
    /// The conversation the request belongs to (original, un-sanitized id).
    pub conversation: String,
    /// The tool the agent wants to use (e.g. `"shell"`).
    pub tool: String,
    /// Human-readable detail shown to the user (e.g. the command).
    pub detail: String,
}

/// The result of resolving a conversation's pending approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalOutcome {
    /// There was a pending request and the user approved it.
    Approved,
    /// There was a pending request and the user denied it.
    Denied,
    /// There was no pending request to resolve.
    Nothing,
}

/// Errors from persisting or loading pending approvals.
#[derive(Debug)]
pub enum ApprovalError {
    /// The backing directory could not be created or written to.
    Io(std::io::Error),
    /// A pending record could not be serialized to JSON.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalError::Io(e) => write!(f, "approval store i/o error: {e}"),
            ApprovalError::Serialize(e) => write!(f, "approval serialize error: {e}"),
        }
    }
}

impl std::error::Error for ApprovalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApprovalError::Io(e) => Some(e),
            ApprovalError::Serialize(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for ApprovalError {
    fn from(e: std::io::Error) -> Self {
        ApprovalError::Io(e)
    }
}

impl From<serde_json::Error> for ApprovalError {
    fn from(e: serde_json::Error) -> Self {
        ApprovalError::Serialize(e)
    }
}

/// A file-backed store of pending approvals, one JSON file per conversation.
#[derive(Clone, Debug)]
pub struct ApprovalStore {
    dir: PathBuf,
}

/// Sanitize a conversation id into a safe filename stem.
///
/// Keeps only ASCII alphanumerics, `-` and `_`; every other character (path
/// separators, `.`, whitespace, ...) is replaced with `_`. This guarantees the
/// result is a single path component with no traversal (`..`) potential. An
/// empty or all-stripped id falls back to a stable default so it still maps to
/// exactly one file.
fn sanitize_conversation(conversation: &str) -> String {
    let mut out = String::with_capacity(conversation.len());
    for ch in conversation.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    // Guard against an empty stem and against `.`/`..`-only stems (which the
    // replacement above already turns into `_`/`__`, but be explicit).
    if out.is_empty() {
        return "_default".to_string();
    }
    out
}

impl ApprovalStore {
    /// Create a store backed by `dir`. The directory is created lazily on the
    /// first [`record`](Self::record).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        ApprovalStore { dir: dir.into() }
    }

    /// The on-disk path for a conversation's pending record.
    fn path_for(&self, conversation: &str) -> PathBuf {
        let mut name = sanitize_conversation(conversation);
        name.push_str(".json");
        self.dir.join(name)
    }

    /// Record (or overwrite) the pending approval for `conversation`.
    ///
    /// A conversation has at most one outstanding request; recording a new one
    /// replaces any existing pending request for that conversation.
    pub fn record(
        &self,
        conversation: &str,
        tool: &str,
        detail: &str,
    ) -> Result<(), ApprovalError> {
        fs::create_dir_all(&self.dir)?;
        let pending = PendingApproval {
            conversation: conversation.to_string(),
            tool: tool.to_string(),
            detail: detail.to_string(),
        };
        let json = serde_json::to_string_pretty(&pending)?;
        fs::write(self.path_for(conversation), json)?;
        Ok(())
    }

    /// Read back the pending approval for `conversation`, if any.
    ///
    /// Returns `None` when there is no pending request, or when the stored file
    /// is missing/unreadable/corrupt (a corrupt file is treated as "no pending
    /// request" rather than surfacing an error to the chat flow).
    pub fn pending(&self, conversation: &str) -> Option<PendingApproval> {
        let raw = fs::read_to_string(self.path_for(conversation)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Resolve and consume the pending approval for `conversation`.
    ///
    /// If a request was pending it is removed and the corresponding outcome is
    /// returned ([`ApprovalOutcome::Approved`] when `approved`, otherwise
    /// [`ApprovalOutcome::Denied`]). If nothing was pending,
    /// [`ApprovalOutcome::Nothing`] is returned and no file is touched.
    pub fn resolve(&self, conversation: &str, approved: bool) -> ApprovalOutcome {
        let path = self.path_for(conversation);
        if !path.exists() {
            return ApprovalOutcome::Nothing;
        }
        // Best-effort removal; even if removal fails we still report the
        // decision so the user isn't left stuck. A stale file would be
        // overwritten by the next `record`.
        let _ = fs::remove_file(&path);
        if approved {
            ApprovalOutcome::Approved
        } else {
            ApprovalOutcome::Denied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A unique temp directory for an isolated test run (no external crates).
    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut dir = std::env::temp_dir();
        dir.push(format!("codel00p-approvals-{tag}-{nanos}"));
        dir
    }

    #[test]
    fn record_then_pending_returns_it() {
        let store = ApprovalStore::new(temp_dir("record"));
        store.record("C123", "shell", "rm -rf build").unwrap();

        let pending = store.pending("C123").expect("pending present");
        assert_eq!(pending.conversation, "C123");
        assert_eq!(pending.tool, "shell");
        assert_eq!(pending.detail, "rm -rf build");
    }

    #[test]
    fn pending_is_none_when_nothing_recorded() {
        let store = ApprovalStore::new(temp_dir("none"));
        assert!(store.pending("C123").is_none());
    }

    #[test]
    fn resolve_true_approves_and_clears() {
        let store = ApprovalStore::new(temp_dir("approve"));
        store.record("C1", "shell", "ls").unwrap();

        assert_eq!(store.resolve("C1", true), ApprovalOutcome::Approved);
        // The pending request is consumed.
        assert!(store.pending("C1").is_none());
        // Resolving again finds nothing.
        assert_eq!(store.resolve("C1", true), ApprovalOutcome::Nothing);
    }

    #[test]
    fn resolve_false_denies_and_clears() {
        let store = ApprovalStore::new(temp_dir("deny"));
        store.record("C1", "shell", "ls").unwrap();

        assert_eq!(store.resolve("C1", false), ApprovalOutcome::Denied);
        assert!(store.pending("C1").is_none());
    }

    #[test]
    fn resolve_when_none_returns_nothing() {
        let store = ApprovalStore::new(temp_dir("nothing"));
        assert_eq!(
            store.resolve("never-recorded", true),
            ApprovalOutcome::Nothing
        );
    }

    #[test]
    fn record_overwrites_previous_pending() {
        let store = ApprovalStore::new(temp_dir("overwrite"));
        store.record("C1", "shell", "first").unwrap();
        store.record("C1", "fs", "second").unwrap();

        let pending = store.pending("C1").unwrap();
        assert_eq!(pending.tool, "fs");
        assert_eq!(pending.detail, "second");
    }

    #[test]
    fn unsafe_conversation_ids_are_sanitized() {
        // A traversal attempt must stay inside the store directory and map to a
        // plain `.json` file there, never to a parent path.
        let dir = temp_dir("traversal");
        let store = ApprovalStore::new(&dir);
        let hostile = "../../etc/passwd";

        store.record(hostile, "shell", "pwn").unwrap();

        // The original id round-trips for the same hostile string.
        let pending = store.pending(hostile).expect("pending present");
        assert_eq!(pending.conversation, hostile);

        // Exactly one file was created, directly inside the store dir, with a
        // sanitized name containing no path separators or `.` traversal.
        let entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries.len(), 1, "exactly one record file: {entries:?}");
        let name = &entries[0];
        assert!(name.ends_with(".json"));
        let stem = name.trim_end_matches(".json");
        assert!(
            stem.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "stem must be sanitized: {stem}"
        );
    }

    #[test]
    fn distinct_conversations_are_independent() {
        let store = ApprovalStore::new(temp_dir("independent"));
        store.record("A", "shell", "a").unwrap();
        store.record("B", "shell", "b").unwrap();

        assert_eq!(store.resolve("A", true), ApprovalOutcome::Approved);
        // B is untouched.
        assert_eq!(store.pending("B").unwrap().detail, "b");
        assert_eq!(store.resolve("B", false), ApprovalOutcome::Denied);
    }

    #[test]
    fn empty_conversation_id_maps_to_a_stable_file() {
        let store = ApprovalStore::new(temp_dir("empty"));
        store.record("", "shell", "x").unwrap();
        assert_eq!(store.pending("").unwrap().detail, "x");
        assert_eq!(store.resolve("", true), ApprovalOutcome::Approved);
    }
}
