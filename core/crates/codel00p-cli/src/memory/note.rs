//! `codel00p memory note` — inspect and append to the capped curated memory
//! layer (NOTES.md + USER.md) in the active agent home.
//!
//! This is the scriptable surface over the same notes the agent maintains with
//! the harness `note` tool. Appends enforce the per-file char caps (no
//! truncation); `--show` prints the current notes.

use codel00p_harness::{
    NOTES_CHAR_CAP, NOTES_FILE, NoteTarget, USER_CHAR_CAP, USER_FILE, append_note, read_note_file,
};

use crate::config::{CliConfig, CliResult};

/// `codel00p memory note [--user] <text>` appends a note; `memory note --show`
/// prints the current curated memory. `--user` targets `USER.md` (facts about
/// the user); the default targets `NOTES.md` (the agent's own notes).
pub(super) fn memory_note(_config: CliConfig, args: &[String]) -> CliResult<String> {
    let home = crate::settings::home_dir();

    let mut target = NoteTarget::Agent;
    let mut show = false;
    let mut text_parts: Vec<String> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--user" => {
                target = NoteTarget::User;
                index += 1;
            }
            "--show" => {
                show = true;
                index += 1;
            }
            // Everything else is treated as the note text (joined with spaces),
            // so `memory note remember to run cargo fmt` works without quoting.
            other => {
                text_parts.push(other.to_string());
                index += 1;
            }
        }
    }

    if show {
        if !text_parts.is_empty() {
            return Err("memory note --show takes no note text".to_string());
        }
        return Ok(render_current(&home));
    }

    if text_parts.is_empty() {
        return Err(
            "memory note expects note text (or --show). Usage: memory note [--user] <text>"
                .to_string(),
        );
    }

    let text = text_parts.join(" ");
    let contents = append_note(&home, target, &text)?;
    let cap = target.char_cap();
    Ok(format!(
        "added to {} ({}/{} chars)",
        target.file_name(),
        contents.chars().count(),
        cap
    ))
}

/// Render the current curated memory for `--show`: each file with its char usage
/// against the cap, or a hint when both are empty.
fn render_current(home: &std::path::Path) -> String {
    let notes = read_note_file(home, NoteTarget::Agent);
    let user = read_note_file(home, NoteTarget::User);
    if notes.is_empty() && user.is_empty() {
        return "(no curated memory — add with `memory note <text>` or `memory note --user <text>`)"
            .to_string();
    }

    let mut output = String::new();
    output.push_str(&format!(
        "# {NOTES_FILE} ({}/{NOTES_CHAR_CAP} chars)\n",
        notes.chars().count()
    ));
    if notes.is_empty() {
        output.push_str("(empty)\n");
    } else {
        output.push_str(&notes);
        output.push('\n');
    }
    output.push_str(&format!(
        "\n# {USER_FILE} ({}/{USER_CHAR_CAP} chars)\n",
        user.chars().count()
    ));
    if user.is_empty() {
        output.push_str("(empty)\n");
    } else {
        output.push_str(&user);
        output.push('\n');
    }
    output
}
