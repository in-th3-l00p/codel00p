//! Per-agent capped curated memory — the always-in-context notes layer (Hermes'
//! MEMORY.md / USER.md model), distinct from the searchable memory DB.
//!
//! Two small files live in the agent home (`CODEL00P_HOME`):
//! - [`NOTES_FILE`] (`NOTES.md`): the agent's own durable notes — environment
//!   facts, conventions, lessons. Capped at [`NOTES_CHAR_CAP`] chars.
//! - [`USER_FILE`] (`USER.md`): facts about the user — preferences, style.
//!   Capped at [`USER_CHAR_CAP`] chars.
//!
//! Their non-empty contents are rendered into a single compact system block that
//! is injected every turn (see the provider adapter), so the caps keep the
//! always-paid token cost bounded. The [`NoteTool`] lets the agent curate the two
//! files itself; the caps are enforced on write — on overflow the tool returns an
//! error instructing the agent to consolidate (Hermes semantics: never
//! auto-truncate).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{errors::HarnessError, tool_result::ToolResult, tools::Tool, workspace::Workspace};
use codel00p_protocol::PermissionScope;

/// File name of the agent's own durable notes (env facts, conventions, lessons).
pub const NOTES_FILE: &str = "NOTES.md";
/// File name of the facts-about-the-user notes (preferences, style).
pub const USER_FILE: &str = "USER.md";

/// Character cap for `NOTES.md`. Small on purpose: this is paid every turn.
pub const NOTES_CHAR_CAP: usize = 2000;
/// Character cap for `USER.md`. Small on purpose: this is paid every turn.
pub const USER_CHAR_CAP: usize = 1200;

/// Which curated-memory file a [`NoteTool`] action targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteTarget {
    /// The agent's own durable notes (`NOTES.md`).
    Agent,
    /// Facts about the user (`USER.md`).
    User,
}

impl NoteTarget {
    /// The file name backing this target.
    pub fn file_name(self) -> &'static str {
        match self {
            NoteTarget::Agent => NOTES_FILE,
            NoteTarget::User => USER_FILE,
        }
    }

    /// The character cap that applies to this target.
    pub fn char_cap(self) -> usize {
        match self {
            NoteTarget::Agent => NOTES_CHAR_CAP,
            NoteTarget::User => USER_CHAR_CAP,
        }
    }

    /// Parse a target from the tool's `target` field (defaults to the agent's own
    /// notes when absent).
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("agent") | Some("notes") => Ok(NoteTarget::Agent),
            Some("user") => Ok(NoteTarget::User),
            Some(other) => Err(format!(
                "unknown target {other:?}: use \"agent\" (NOTES.md) or \"user\" (USER.md)"
            )),
        }
    }
}

/// Read a curated-memory file from `home`, returning its trimmed contents (empty
/// string if the file is missing or unreadable).
pub fn read_note_file(home: &Path, target: NoteTarget) -> String {
    std::fs::read_to_string(home.join(target.file_name()))
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

/// Assemble the always-injected curated-memory system block from the two files in
/// `home`. Returns `None` when both files are empty, so no block is injected.
///
/// The block is a small header followed by only the non-empty sections, keeping
/// it compact — it is paid on every turn.
pub fn assemble_block(home: &Path) -> Option<String> {
    let notes = read_note_file(home, NoteTarget::Agent);
    let user = read_note_file(home, NoteTarget::User);
    if notes.is_empty() && user.is_empty() {
        return None;
    }

    let mut block = String::from(
        "# Curated memory (what I durably know)\nMy capped, always-in-context notes. \
         Keep them accurate and consolidated; curate with the `note` tool.\n",
    );
    if !notes.is_empty() {
        block.push_str("\n## Notes (env, conventions, lessons)\n");
        block.push_str(&notes);
        block.push('\n');
    }
    if !user.is_empty() {
        block.push_str("\n## About the user\n");
        block.push_str(&user);
        block.push('\n');
    }
    Some(block)
}

/// Append `text` to the target file under `home`, enforcing the per-file char
/// cap. Shared by the CLI `memory note` surface and the [`NoteTool`].
///
/// On overflow the contents are left unchanged and an error is returned asking
/// the caller to consolidate — never auto-truncated.
pub fn append_note(home: &Path, target: NoteTarget, text: &str) -> Result<String, String> {
    let addition = text.trim();
    if addition.is_empty() {
        return Err("note text is empty".to_string());
    }
    let current = read_note_file(home, target);
    let next = if current.is_empty() {
        addition.to_string()
    } else {
        format!("{current}\n{addition}")
    };
    write_capped(home, target, &next)
}

/// Replace the first substring match of `find` with `replacement` in the target
/// file, enforcing the char cap. Errors if `find` is not present.
pub fn replace_note(
    home: &Path,
    target: NoteTarget,
    find: &str,
    replacement: &str,
) -> Result<String, String> {
    if find.is_empty() {
        return Err("`find` text is empty".to_string());
    }
    let current = read_note_file(home, target);
    if !current.contains(find) {
        return Err(format!(
            "no match for {find:?} in {} — nothing replaced",
            target.file_name()
        ));
    }
    let next = current.replacen(find, replacement, 1);
    write_capped(home, target, next.trim())
}

/// Remove the first substring match of `find` from the target file. Errors if
/// `find` is not present.
pub fn remove_note(home: &Path, target: NoteTarget, find: &str) -> Result<String, String> {
    if find.is_empty() {
        return Err("`find` text is empty".to_string());
    }
    let current = read_note_file(home, target);
    if !current.contains(find) {
        return Err(format!(
            "no match for {find:?} in {} — nothing removed",
            target.file_name()
        ));
    }
    // Collapse any blank line left behind so the file stays compact.
    let next: String = current
        .replacen(find, "", 1)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    write_capped(home, target, next.trim())
}

/// Write `contents` to the target file under `home`, refusing (with an
/// actionable error) when it would exceed the cap. Never truncates.
fn write_capped(home: &Path, target: NoteTarget, contents: &str) -> Result<String, String> {
    let cap = target.char_cap();
    let len = contents.chars().count();
    if len > cap {
        return Err(format!(
            "{} would be {len} chars, over the {cap}-char cap. Consolidate existing notes \
             (replace/remove redundant lines) before adding more — this layer is intentionally \
             small because it is in context every turn.",
            target.file_name()
        ));
    }
    let path = home.join(target.file_name());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // Trailing newline keeps the file tidy; the stored body is the trimmed form.
    std::fs::write(&path, format!("{contents}\n")).map_err(|error| error.to_string())?;
    Ok(contents.to_string())
}

/// Agent-writable curated memory: the `note` tool. Lets the agent maintain its
/// own capped notes (`NOTES.md`) and the user-profile notes (`USER.md`) with
/// `add` / `replace` / `remove` actions, enforcing the char caps.
///
/// The tool carries the agent home directory, so writes always land in the
/// active agent's home (`CODEL00P_HOME`) and notes are per-agent automatically —
/// independent of the project workspace the harness operates on.
pub struct NoteTool {
    home: PathBuf,
}

impl NoteTool {
    /// Build a note tool that writes curated notes under `home` (the agent home,
    /// i.e. `CODEL00P_HOME`).
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    fn invalid(&self, message: impl Into<String>) -> HarnessError {
        HarnessError::InvalidToolInput {
            name: self.name().to_string(),
            message: message.into(),
        }
    }
}

#[async_trait]
impl Tool for NoteTool {
    fn name(&self) -> &str {
        "note"
    }

    fn description(&self) -> &str {
        "Curate your capped, always-in-context notes. `NOTES.md` holds your own durable \
         knowledge (environment facts, conventions, lessons); `USER.md` holds facts about \
         the user (preferences, style) when you set `target: \"user\"`. Actions: \"add\" \
         appends `text`; \"replace\" swaps the first substring matching `find` with \
         `replacement`; \"remove\" deletes the first substring matching `find`. The files \
         are character-capped — if a write would overflow, the call fails and you must \
         consolidate (replace/remove redundant notes) instead of letting it grow. Notes are \
         per-agent and persist across sessions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "replace", "remove"]
                },
                "text": {
                    "type": "string",
                    "description": "Text to append (action=add)."
                },
                "find": {
                    "type": "string",
                    "description": "Substring to match (action=replace/remove)."
                },
                "replacement": {
                    "type": "string",
                    "description": "Replacement text (action=replace)."
                },
                "target": {
                    "type": "string",
                    "enum": ["agent", "user"],
                    "description": "Which file: \"agent\" (NOTES.md, default) or \"user\" (USER.md)."
                }
            },
            "required": ["action"]
        })
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        // Curating the always-in-context notes layer is a memory write, not a
        // workspace edit — it touches the agent home, never the project tree.
        PermissionScope::MemoryWrite
    }

    async fn execute(
        &self,
        _workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let action = input
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| self.invalid("`action` is required"))?;
        let target = NoteTarget::parse(input.get("target").and_then(Value::as_str))
            .map_err(|message| self.invalid(message))?;

        let outcome = match action {
            "add" => {
                let text = input
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.invalid("`text` is required for action=add"))?;
                append_note(&self.home, target, text)
            }
            "replace" => {
                let find = input
                    .get("find")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.invalid("`find` is required for action=replace"))?;
                let replacement = input
                    .get("replacement")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.invalid("`replacement` is required for action=replace"))?;
                replace_note(&self.home, target, find, replacement)
            }
            "remove" => {
                let find = input
                    .get("find")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.invalid("`find` is required for action=remove"))?;
                remove_note(&self.home, target, find)
            }
            other => {
                return Err(self.invalid(format!(
                    "unknown action {other:?}: use \"add\", \"replace\", or \"remove\""
                )));
            }
        };

        let contents = outcome.map_err(|message| HarnessError::ToolFailed {
            name: self.name().to_string(),
            message,
        })?;

        Ok(ToolResult::json(json!({
            "file": target.file_name(),
            "action": action,
            "chars": contents.chars().count(),
            "cap": target.char_cap(),
            "contents": contents,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codel00p-curated-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn add_replace_remove_round_trip_on_notes() {
        let home = tmp();
        let workspace = Workspace::new(&home).unwrap();
        let tool = NoteTool::new(&home);

        tool.execute(&workspace, json!({ "action": "add", "text": "uses pnpm" }))
            .await
            .unwrap();
        assert_eq!(read_note_file(&home, NoteTarget::Agent), "uses pnpm");

        tool.execute(
            &workspace,
            json!({ "action": "add", "text": "tests live in core/" }),
        )
        .await
        .unwrap();
        assert!(read_note_file(&home, NoteTarget::Agent).contains("tests live in core/"));

        tool.execute(
            &workspace,
            json!({ "action": "replace", "find": "pnpm", "replacement": "cargo" }),
        )
        .await
        .unwrap();
        let after = read_note_file(&home, NoteTarget::Agent);
        assert!(after.contains("uses cargo"));
        assert!(!after.contains("pnpm"));

        tool.execute(
            &workspace,
            json!({ "action": "remove", "find": "uses cargo" }),
        )
        .await
        .unwrap();
        let after = read_note_file(&home, NoteTarget::Agent);
        assert!(!after.contains("cargo"));
        assert!(after.contains("tests live in core/"));
    }

    #[tokio::test]
    async fn target_user_writes_user_file() {
        let home = tmp();
        let workspace = Workspace::new(&home).unwrap();
        let tool = NoteTool::new(&home);

        tool.execute(
            &workspace,
            json!({ "action": "add", "text": "prefers terse answers", "target": "user" }),
        )
        .await
        .unwrap();

        assert_eq!(
            read_note_file(&home, NoteTarget::User),
            "prefers terse answers"
        );
        // The agent's own notes are untouched.
        assert_eq!(read_note_file(&home, NoteTarget::Agent), "");
    }

    #[tokio::test]
    async fn cap_overflow_errors_without_truncation() {
        let home = tmp();
        let workspace = Workspace::new(&home).unwrap();
        let tool = NoteTool::new(&home);

        let big = "x".repeat(NOTES_CHAR_CAP + 50);
        let result = tool
            .execute(&workspace, json!({ "action": "add", "text": big }))
            .await;
        assert!(result.is_err(), "over-cap add should fail");
        // Nothing was written — no truncation.
        assert_eq!(read_note_file(&home, NoteTarget::Agent), "");
    }

    #[test]
    fn assemble_block_only_includes_non_empty_sections() {
        let home = tmp();
        assert!(assemble_block(&home).is_none());

        append_note(&home, NoteTarget::Agent, "fact one").unwrap();
        let block = assemble_block(&home).unwrap();
        assert!(block.contains("Curated memory"));
        assert!(block.contains("fact one"));
        assert!(!block.contains("About the user"));

        append_note(&home, NoteTarget::User, "likes Rust").unwrap();
        let block = assemble_block(&home).unwrap();
        assert!(block.contains("About the user"));
        assert!(block.contains("likes Rust"));
    }
}
