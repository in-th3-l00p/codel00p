//! Workspace and error helpers shared by the editing tools.
//!
//! Every mutating tool routes its file existence checks, destination guards, and
//! reads through these so boundary enforcement and backend I/O stay in one place
//! and the error wording stays consistent across tools.

use std::path::PathBuf;

use serde_json::Value;

use crate::{errors::HarnessError, workspace::Workspace};

/// Construct the canonical `ToolFailed` error these tools return.
pub(super) fn tool_failed(name: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::ToolFailed {
        name: name.to_string(),
        message: message.into(),
    }
}

/// Read an optional boolean flag from tool input, defaulting to `false`.
pub(super) fn optional_bool(input: &Value, key: &str) -> bool {
    input.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Read the raw bytes of an existing source file (binary-safe), erroring with a
/// clear message if it is missing. Boundary enforcement and I/O are routed
/// through the workspace facade / backend.
pub(super) fn read_source_file(
    workspace: &Workspace,
    tool_name: &str,
    from: &str,
) -> Result<Vec<u8>, HarnessError> {
    require_existing_file(workspace, tool_name, from)?;
    workspace.read_bytes(from)
}

/// Reject an existing destination unless `overwrite` is requested, so neither
/// move nor copy silently clobbers a file.
pub(super) fn ensure_dest_free(
    workspace: &Workspace,
    tool_name: &str,
    to: &str,
    overwrite: bool,
) -> Result<(), HarnessError> {
    if !overwrite && workspace.exists(to)? {
        return Err(tool_failed(
            tool_name,
            format!("destination already exists: {to} (set `overwrite` to true to replace it)"),
        ));
    }
    Ok(())
}

/// Require that `path` names an existing regular file in the workspace,
/// returning a clear "file does not exist" error otherwise. I/O is routed
/// through the workspace facade (and thus the backend).
pub(super) fn require_existing_file(
    workspace: &Workspace,
    tool_name: &str,
    path: &str,
) -> Result<(), HarnessError> {
    if !workspace.is_file(path)? {
        return Err(tool_failed(
            tool_name,
            format!("file does not exist: {path}"),
        ));
    }
    Ok(())
}

/// Require an existing file and return its canonical real path, used purely as a
/// stable identity key so multiple patch changes to the same file (however the
/// relative path is spelled) compose. Escape / unsafe-path errors propagate as
/// before; a path that resolves but is not a regular file yields the same
/// "file does not exist" error.
pub(super) fn existing_file_key(
    workspace: &Workspace,
    tool_name: &str,
    path: &str,
) -> Result<PathBuf, HarnessError> {
    let resolved = workspace.resolve_for_create(path)?;
    if !workspace.is_file(path)? {
        return Err(tool_failed(
            tool_name,
            format!("file does not exist: {path}"),
        ));
    }
    // Canonicalize for a stable identity key (collapses `./a` vs `a`, symlinks).
    Ok(resolved.canonicalize().unwrap_or(resolved))
}
