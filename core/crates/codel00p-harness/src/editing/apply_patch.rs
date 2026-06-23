//! The `apply_patch` tool: the model-facing editor over the pure [`super::patch`]
//! engine.
//!
//! This layer owns three things and nothing else: the tool's schema/description,
//! the tolerant normalization of the call into a list of changes, and the atomic
//! batch application against the workspace. The actual find/replace lives in
//! `super::patch`, so this file reads as orchestration.

use std::{collections::BTreeMap, path::PathBuf};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, tools::Tool, workspace::Workspace};

use super::{patch::apply_change, support::existing_file_key, support::tool_failed};

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply string replacements to workspace files. Pass a `changes` array, each \
         item a `{path, find, replace}` object (optional `replace_all`). Matches the \
         `find` text exactly when possible, otherwise falls back to whitespace-, \
         line-ending-, and indentation-tolerant matching while preserving the rest of \
         the file byte-for-byte. By default a `find` that occurs more than once is \
         rejected; set `replace_all` to true to replace every occurrence. Multiple \
         changes are applied as one atomic batch: nothing is written unless every \
         change matches, and several changes targeting the same file are applied in \
         order, each seeing the result of the previous one. A single change may also \
         be passed flat as a top-level `{path, find, replace}` without the wrapper."
    }

    fn input_schema(&self) -> Value {
        // `changes` is documented (and preferred) but not declared `required`, so a
        // single flat `{path, find, replace}` call — a common shape from smaller
        // models — reaches `execute` and is normalized there instead of being
        // pre-rejected by schema validation with a confusing message.
        json!({
            "type": "object",
            "properties": {
                // `changes` is documented as an array (see `items`) but its type
                // is intentionally not pinned, so a lone change object — a shape
                // smaller models sometimes emit — is normalized in `execute`
                // rather than pre-rejected by schema validation.
                "changes": {
                    "items": {
                        "type": "object",
                        "required": ["path", "find", "replace"],
                        "properties": {
                            "path": { "type": "string" },
                            "find": { "type": "string" },
                            "replace": { "type": "string" },
                            "replace_all": { "type": "boolean" }
                        }
                    }
                },
                "path": { "type": "string" },
                "find": { "type": "string" },
                "replace": { "type": "string" },
                "replace_all": { "type": "boolean" }
            }
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::WorkspaceWrite
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        // Normalize the call into a list of changes first. This tolerates the
        // common smaller-model deviations from the canonical
        // `{changes:[{path,find,replace}]}` shape — a single flat
        // `{path,find,replace}` call, a top-level `path` shared by changes that
        // omit it, and well-known field aliases (`file`, `old_string`,
        // `new_string`, …) — so a recoverable mistake becomes a successful edit
        // rather than a "missing string field `path`" loop.
        let changes = normalize_changes(self.name(), &input)?;

        // Apply every change in memory before touching disk so the batch is atomic
        // (no partial writes on a later mismatch) and so multiple changes to the
        // same file compose: each change sees the result of the previous one.
        //
        // The map is keyed on each file's canonical real path so two changes to
        // the same file (regardless of how the relative path is spelled) compose,
        // exactly as before. Each entry remembers the relative path to write back
        // through the workspace facade (which delegates I/O to the backend).
        let mut contents: BTreeMap<PathBuf, (String, String)> = BTreeMap::new();
        let mut summaries = Vec::new();
        for change in &changes {
            let NormalizedChange {
                path,
                find,
                replace,
                replace_all,
            } = change;
            let path = path.as_str();

            let key = existing_file_key(workspace, self.name(), path)?;
            let current = match contents.get(&key) {
                Some((_, existing)) => existing.clone(),
                None => workspace.read_utf8(path)?,
            };

            let outcome = apply_change(&current, find, replace, *replace_all)
                .map_err(|message| tool_failed(self.name(), format!("{path}: {message}")))?;
            summaries.push(json!({
                "path": path,
                "replacements": outcome.replacements,
                "bytes_written": outcome.patched.len(),
                "strategy": outcome.strategy,
            }));
            contents.insert(key, (path.to_string(), outcome.patched));
        }

        for (rel_path, patched) in contents.values() {
            workspace.write(rel_path, patched)?;
        }

        Ok(ToolResult::json(json!({
            "operation": "patched",
            "changes": summaries,
        })))
    }
}

/// One `apply_patch` change after normalization: a workspace-relative path and
/// the find/replace pair (plus the `replace_all` flag).
struct NormalizedChange {
    path: String,
    find: String,
    replace: String,
    replace_all: bool,
}

/// Field aliases tolerated for each change field. A smaller model that reaches
/// for a near-synonym (the shape it learned from a differently-named edit tool)
/// still lands a valid edit instead of looping on "missing string field `path`".
const PATH_KEYS: &[&str] = &["path", "file", "file_path", "filepath", "filename"];
const FIND_KEYS: &[&str] = &["find", "search", "old", "old_string", "old_str", "old_text"];
const REPLACE_KEYS: &[&str] = &[
    "replace",
    "replacement",
    "new",
    "new_string",
    "new_str",
    "new_text",
];
const REPLACE_ALL_KEYS: &[&str] = &["replace_all", "replaceAll"];

/// Normalize an `apply_patch` call into a list of changes, tolerating the common
/// deviations smaller models make from the canonical
/// `{ "changes": [{ "path", "find", "replace" }] }` shape:
///
/// * a single flat `{ "path", "find", "replace" }` with no `changes` wrapper;
/// * a top-level `path` (and/or `find`/`replace`) shared by every change that
///   omits its own — the "name the file once, then list edits" shape;
/// * well-known field aliases (`file`, `old_string`, `new_string`, …).
///
/// Canonical calls are unaffected. Errors stay actionable and name the field.
fn normalize_changes(tool: &str, input: &Value) -> Result<Vec<NormalizedChange>, HarnessError> {
    let default_path = first_str(input, PATH_KEYS);
    let default_find = first_str(input, FIND_KEYS);
    let default_replace = first_str(input, REPLACE_KEYS);
    let default_replace_all = first_bool(input, REPLACE_ALL_KEYS);

    // The change objects, if any. A `changes` that is a single object (not an
    // array) is accepted as a one-element list; with no `changes` at all, the
    // whole input is treated as one flat change.
    let raw_changes: Vec<&Value> = match input.get("changes") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(object @ Value::Object(_)) => vec![object],
        _ => vec![input],
    };
    if raw_changes.is_empty() {
        return Err(invalid_input(
            tool,
            "`changes` must not be empty".to_string(),
        ));
    }

    let mut changes = Vec::with_capacity(raw_changes.len());
    for change in raw_changes {
        // Resolve each field from the change itself first, then fall back to the
        // shared top-level default (so a single top-level `path` reaches changes
        // that omit it). In the flat form the change *is* the input, so the
        // per-change lookup and the default coincide harmlessly.
        let path = first_str(change, PATH_KEYS)
            .or(default_path)
            .ok_or_else(|| missing_field(tool, "path"))?;
        let find = first_str(change, FIND_KEYS)
            .or(default_find)
            .ok_or_else(|| missing_field(tool, "find"))?;
        // An empty `replace` is valid (a deletion), so a present-but-empty value
        // must not fall through to the default; only a missing key does.
        let replace = first_str(change, REPLACE_KEYS)
            .or(default_replace)
            .ok_or_else(|| missing_field(tool, "replace"))?;
        let replace_all = first_bool(change, REPLACE_ALL_KEYS)
            .or(default_replace_all)
            .unwrap_or(false);

        if find.is_empty() {
            return Err(invalid_input(tool, "`find` must not be empty".to_string()));
        }
        changes.push(NormalizedChange {
            path: path.to_string(),
            find: find.to_string(),
            replace: replace.to_string(),
            replace_all,
        });
    }
    Ok(changes)
}

/// First present string value among `keys`.
fn first_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

/// First present boolean value among `keys`.
fn first_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn invalid_input(tool: &str, message: String) -> HarnessError {
    HarnessError::InvalidToolInput {
        name: tool.to_string(),
        message,
    }
}

fn missing_field(tool: &str, field: &str) -> HarnessError {
    invalid_input(tool, format!("missing string field `{field}`"))
}
