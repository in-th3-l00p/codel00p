use std::{collections::BTreeMap, path::PathBuf};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, required_string},
    workspace::Workspace,
};

pub struct CreateFileTool;

#[async_trait]
impl Tool for CreateFileTool {
    fn name(&self) -> &str {
        "create_file"
    }

    fn description(&self) -> &str {
        "Create a UTF-8 file inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
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
        let path = required_string(self.name(), &input, "path")?;
        let content = required_string(self.name(), &input, "content")?;
        if workspace.exists(path)? {
            return Err(tool_failed(
                self.name(),
                format!("file already exists: {path}"),
            ));
        }

        workspace.write(path, content)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "created",
            "bytes_written": content.len(),
        })))
    }
}

pub struct UpdateFileTool;

#[async_trait]
impl Tool for UpdateFileTool {
    fn name(&self) -> &str {
        "update_file"
    }

    fn description(&self) -> &str {
        "Replace a UTF-8 file inside the workspace root. Prefer `apply_patch` for \
         targeted edits; use this only to replace an entire file."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
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
        let path = required_string(self.name(), &input, "path")?;
        let content = required_string(self.name(), &input, "content")?;
        require_existing_file(workspace, self.name(), path)?;
        workspace.write(path, content)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "updated",
            "bytes_written": content.len(),
        })))
    }
}

pub struct DeleteFileTool;

#[async_trait]
impl Tool for DeleteFileTool {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "Delete a file inside the workspace root."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" }
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
        let path = required_string(self.name(), &input, "path")?;
        require_existing_file(workspace, self.name(), path)?;
        workspace.delete(path)?;

        Ok(ToolResult::json(json!({
            "path": path,
            "operation": "deleted",
        })))
    }
}

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply string replacements to workspace files. Matches the `find` text \
         exactly when possible, otherwise falls back to whitespace-, line-ending-, \
         and indentation-tolerant matching while preserving the rest of the file \
         byte-for-byte. By default a `find` that occurs more than once is rejected; \
         set `replace_all` to true to replace every occurrence. Multiple changes are \
         applied as one atomic batch: nothing is written unless every change matches, \
         and several changes targeting the same file are applied in order, each seeing \
         the result of the previous one."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["changes"],
            "properties": {
                "changes": {
                    "type": "array",
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
                }
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
        let changes = input
            .get("changes")
            .and_then(Value::as_array)
            .ok_or_else(|| HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "missing array field `changes`".to_string(),
            })?;
        if changes.is_empty() {
            return Err(HarnessError::InvalidToolInput {
                name: self.name().to_string(),
                message: "`changes` must not be empty".to_string(),
            });
        }

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
        for change in changes {
            let path = required_string(self.name(), change, "path")?;
            let find = required_string(self.name(), change, "find")?;
            let replace = required_string(self.name(), change, "replace")?;
            let replace_all = change
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if find.is_empty() {
                return Err(HarnessError::InvalidToolInput {
                    name: self.name().to_string(),
                    message: "`find` must not be empty".to_string(),
                });
            }

            let key = existing_file_key(workspace, self.name(), path)?;
            let current = match contents.get(&key) {
                Some((_, existing)) => existing.clone(),
                None => workspace.read_utf8(path)?,
            };

            let outcome = apply_change(&current, find, replace, replace_all)
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

pub struct MoveFileTool;

#[async_trait]
impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "move_file"
    }

    fn description(&self) -> &str {
        "Move (rename) a file inside the workspace root. Binary-safe and \
         workspace-bounded. Fails if `from` is missing or `to` already exists \
         unless `overwrite` is true. Implemented as copy-then-delete through the \
         execution backend."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["from", "to"],
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "overwrite": { "type": "boolean" }
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
        let from = required_string(self.name(), &input, "from")?;
        let to = required_string(self.name(), &input, "to")?;
        let overwrite = optional_bool(&input, "overwrite");

        let bytes = read_source_file(workspace, self.name(), from)?;
        ensure_dest_free(workspace, self.name(), to, overwrite)?;

        // copy-then-delete so the move flows through the same boundary-checked,
        // backend-aware write/delete path as the other editing tools.
        workspace.write(to, &bytes)?;
        workspace.delete(from)?;

        Ok(ToolResult::json(json!({
            "from": from,
            "to": to,
            "operation": "moved",
            "bytes_written": bytes.len(),
        })))
    }
}

pub struct CopyFileTool;

#[async_trait]
impl Tool for CopyFileTool {
    fn name(&self) -> &str {
        "copy_file"
    }

    fn description(&self) -> &str {
        "Copy a file inside the workspace root, leaving the source intact. \
         Binary-safe and workspace-bounded. Fails if `from` is missing or `to` \
         already exists unless `overwrite` is true."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["from", "to"],
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "overwrite": { "type": "boolean" }
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
        let from = required_string(self.name(), &input, "from")?;
        let to = required_string(self.name(), &input, "to")?;
        let overwrite = optional_bool(&input, "overwrite");

        let bytes = read_source_file(workspace, self.name(), from)?;
        ensure_dest_free(workspace, self.name(), to, overwrite)?;

        workspace.write(to, &bytes)?;

        Ok(ToolResult::json(json!({
            "from": from,
            "to": to,
            "operation": "copied",
            "bytes_written": bytes.len(),
        })))
    }
}

fn optional_bool(input: &Value, key: &str) -> bool {
    input.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Read the raw bytes of an existing source file (binary-safe), erroring with a
/// clear message if it is missing. Boundary enforcement and I/O are routed
/// through the workspace facade / backend.
fn read_source_file(
    workspace: &Workspace,
    tool_name: &str,
    from: &str,
) -> Result<Vec<u8>, HarnessError> {
    require_existing_file(workspace, tool_name, from)?;
    workspace.read_bytes(from)
}

/// Reject an existing destination unless `overwrite` is requested, so neither
/// tool silently clobbers a file.
fn ensure_dest_free(
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
/// returning the same "file does not exist" error as before otherwise. I/O is
/// routed through the workspace facade (and thus the backend).
fn require_existing_file(
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
/// "file does not exist" error the tool reported previously.
fn existing_file_key(
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

fn tool_failed(name: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::ToolFailed {
        name: name.to_string(),
        message: message.into(),
    }
}

/// Result of successfully applying a single change to a file's contents.
struct ChangeOutcome {
    patched: String,
    replacements: usize,
    strategy: &'static str,
}

/// A located occurrence of the `find` text within the original contents,
/// expressed as a byte range plus the exact replacement text to splice in.
struct Located {
    /// Half-open byte range `[start, end)` in the original contents.
    range: std::ops::Range<usize>,
    /// The text to write in place of `range`. Usually the caller's `replace`,
    /// but indentation-tolerant matching re-indents it to the file's actual
    /// leading whitespace so surrounding formatting is preserved.
    replacement: String,
}

/// A matching strategy: locate every occurrence of `find` in the original and
/// return the byte ranges (with replacement text) to splice.
type LocateFn = fn(&str, &str, &str) -> Vec<Located>;

/// Try, in order, the exact fast path then the tolerant fallbacks, and apply
/// the first strategy that yields at least one match.
///
/// Returns the patched contents on success, or a human-readable, actionable
/// error message (without the file path, which the caller prepends).
fn apply_change(
    original: &str,
    find: &str,
    replace: &str,
    replace_all: bool,
) -> Result<ChangeOutcome, String> {
    // Ordered strategies. Each returns the byte ranges (with re-indented
    // replacement text) it matched; the first non-empty wins.
    let strategies: [(&'static str, LocateFn); 4] = [
        ("exact", locate_exact),
        ("line-ending", locate_line_ending),
        ("trailing-whitespace", locate_trailing_whitespace),
        ("indentation", locate_indentation),
    ];

    for (strategy, locate) in strategies {
        let matches = locate(original, find, replace);
        if matches.is_empty() {
            continue;
        }
        if matches.len() > 1 && !replace_all {
            return Err(format!(
                "found {} matches for the `find` text via {strategy} matching; \
                 add surrounding context so it is unique, or set `replace_all` \
                 to true to replace every occurrence",
                matches.len()
            ));
        }
        let replacements = matches.len();
        let patched = splice(original, matches);
        return Ok(ChangeOutcome {
            patched,
            replacements,
            strategy,
        });
    }

    Err(not_found_hint(original, find))
}

/// Replace each located byte range with its replacement text, preserving every
/// byte outside the matched regions exactly. Ranges are sorted/non-overlapping.
fn splice(original: &str, mut matches: Vec<Located>) -> String {
    matches.sort_by_key(|m| m.range.start);
    let mut out = String::with_capacity(original.len());
    let mut cursor = 0usize;
    for located in matches {
        out.push_str(&original[cursor..located.range.start]);
        out.push_str(&located.replacement);
        cursor = located.range.end;
    }
    out.push_str(&original[cursor..]);
    out
}

/// Exact substring matching — the fast path. Preserves prior behaviour.
fn locate_exact(original: &str, find: &str, replace: &str) -> Vec<Located> {
    let mut matches = Vec::new();
    let mut start = 0usize;
    while let Some(found) = original[start..].find(find) {
        let abs = start + found;
        matches.push(Located {
            range: abs..abs + find.len(),
            replacement: replace.to_string(),
        });
        start = abs + find.len();
    }
    matches
}

/// Match line-by-line, ignoring trailing whitespace on every line of both the
/// `find` block and the file. Useful when the model omits or adds trailing
/// spaces. The replacement text is used verbatim.
fn locate_trailing_whitespace(original: &str, find: &str, replace: &str) -> Vec<Located> {
    locate_by_line(original, find, replace, |line| line.trim_end().to_string())
}

/// Match after normalising line endings (CRLF -> LF) on both sides, so a CRLF
/// file can be edited with an LF `find` block (and vice versa).
fn locate_line_ending(original: &str, find: &str, replace: &str) -> Vec<Located> {
    // Only meaningful when the two sides disagree on line endings; otherwise the
    // exact strategy would already have matched.
    locate_by_line(original, find, replace, |line| {
        line.trim_end_matches('\r').to_string()
    })
}

/// Indentation-tolerant matching: match the `find` block ignoring a *uniform*
/// leading-whitespace shift, then re-indent the replacement by the file's actual
/// indentation so the surrounding formatting is preserved.
fn locate_indentation(original: &str, find: &str, replace: &str) -> Vec<Located> {
    let find_lines: Vec<&str> = split_keep_ends(find);
    if find_lines.is_empty() {
        return Vec::new();
    }
    let orig_lines = split_keep_ends(original);

    // Offsets of each original line's start byte.
    let mut offsets = Vec::with_capacity(orig_lines.len() + 1);
    let mut acc = 0usize;
    for line in &orig_lines {
        offsets.push(acc);
        acc += line.len();
    }
    offsets.push(acc);

    let find_keys: Vec<String> = find_lines
        .iter()
        .map(|l| strip_eol(l).trim_start().to_string())
        .collect();

    let mut matches = Vec::new();
    let window = find_lines.len();
    if window == 0 || window > orig_lines.len() {
        return matches;
    }

    let mut i = 0usize;
    while i + window <= orig_lines.len() {
        let mut ok = true;
        for (k, find_key) in find_keys.iter().enumerate() {
            let orig_line = strip_eol(orig_lines[i + k]);
            if orig_line.trim_start() != *find_key {
                ok = false;
                break;
            }
        }
        if ok {
            let start = offsets[i];
            let end = offsets[i + window];
            // Preserve the file's actual indentation: re-indent the replacement
            // by the leading whitespace of the first matched original line.
            let base_indent = leading_whitespace(strip_eol(orig_lines[i]));
            let replacement = reindent(replace, base_indent);
            matches.push(Located {
                range: start..end,
                replacement,
            });
            i += window;
        } else {
            i += 1;
        }
    }
    matches
}

/// Shared line-window matcher: compares the `find` block against the file with a
/// per-line normalising function applied to both sides. The replacement text is
/// used verbatim (line-ending / trailing-whitespace tolerance does not reshape
/// the replacement).
fn locate_by_line(
    original: &str,
    find: &str,
    replace: &str,
    normalize: impl Fn(&str) -> String,
) -> Vec<Located> {
    let find_lines: Vec<&str> = split_keep_ends(find);
    if find_lines.is_empty() {
        return Vec::new();
    }
    let orig_lines = split_keep_ends(original);
    let window = find_lines.len();
    if window > orig_lines.len() {
        return Vec::new();
    }

    let mut offsets = Vec::with_capacity(orig_lines.len() + 1);
    let mut acc = 0usize;
    for line in &orig_lines {
        offsets.push(acc);
        acc += line.len();
    }
    offsets.push(acc);

    let find_keys: Vec<String> = find_lines.iter().map(|l| normalize(strip_eol(l))).collect();

    let mut matches = Vec::new();
    let mut i = 0usize;
    while i + window <= orig_lines.len() {
        let mut ok = true;
        for (k, find_key) in find_keys.iter().enumerate() {
            if normalize(strip_eol(orig_lines[i + k])) != *find_key {
                ok = false;
                break;
            }
        }
        if ok {
            let start = offsets[i];
            let end = offsets[i + window];
            matches.push(Located {
                range: start..end,
                replacement: replace.to_string(),
            });
            i += window;
        } else {
            i += 1;
        }
    }
    matches
}

/// Split into lines while keeping each line's trailing newline (so byte offsets
/// reconstruct the original exactly). A trailing newline does not produce a
/// final empty element.
fn split_keep_ends(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for (idx, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' {
            lines.push(&text[start..=idx]);
            start = idx + 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Strip a trailing `\n` and optional preceding `\r` from a line slice.
fn strip_eol(line: &str) -> &str {
    line.strip_suffix('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .unwrap_or(line)
}

/// The leading-whitespace prefix of a line.
fn leading_whitespace(line: &str) -> &str {
    let end = line
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(line.len());
    &line[..end]
}

/// Re-indent every non-empty line of `replace` so its least-indented line sits
/// at `base_indent`, preserving relative indentation between lines.
fn reindent(replace: &str, base_indent: &str) -> String {
    let lines: Vec<&str> = split_keep_ends(replace);
    if lines.is_empty() {
        return replace.to_string();
    }

    let common = lines
        .iter()
        .filter(|l| !strip_eol(l).trim().is_empty())
        .map(|l| leading_whitespace(strip_eol(l)).len())
        .min()
        .unwrap_or(0);

    let mut out = String::with_capacity(replace.len() + base_indent.len() * lines.len());
    for line in lines {
        let body = strip_eol(line);
        let eol = &line[body.len()..];
        if body.trim().is_empty() {
            out.push_str(body);
        } else {
            out.push_str(base_indent);
            out.push_str(&body[common..]);
        }
        out.push_str(eol);
    }
    out
}

/// Build an actionable not-found message, pointing at the closest near-miss line
/// when one exists so the model can self-correct.
fn not_found_hint(original: &str, find: &str) -> String {
    let find_first = find.lines().next().unwrap_or("").trim();
    if find_first.is_empty() {
        return "find text was not present".to_string();
    }

    // Look for a line that only differs by whitespace.
    let collapsed_target: String = find_first.split_whitespace().collect::<Vec<_>>().join(" ");
    for line in original.lines() {
        let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed == collapsed_target && line.trim() != find_first {
            return format!(
                "find text was not present; a line differs only in whitespace: {:?}",
                line
            );
        }
    }

    // Otherwise surface the closest line by a cheap similarity heuristic.
    let mut best: Option<(usize, &str)> = None;
    for line in original.lines() {
        let score = shared_prefix_len(line.trim(), find_first);
        if score == 0 {
            continue;
        }
        match best {
            Some((best_score, _)) if score <= best_score => {}
            _ => best = Some((score, line)),
        }
    }

    match best {
        Some((_, line)) => format!(
            "find text was not present; closest line is: {:?}",
            line.trim()
        ),
        None => "find text was not present".to_string(),
    }
}

/// Length of the shared leading prefix between two strings (cheap near-miss
/// heuristic).
fn shared_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}
