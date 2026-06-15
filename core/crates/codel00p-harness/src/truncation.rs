//! Tool-result truncation.
//!
//! Verbose tool output (large file reads, command output, MCP results) can flood
//! the model's context window. Before a tool result is recorded into the session
//! (and thus shown to the model on the next turn), the harness caps its size: an
//! over-budget result becomes a head+tail preview with an omission marker, and
//! the full output is written to a temp file referenced in the marker so nothing
//! is lost. This mirrors what mature agents do (e.g. Claude Code persists large
//! Bash output to disk and shows a preview).

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Caps the size of a tool result recorded for the model.
#[derive(Clone, Copy, Debug)]
pub struct ToolOutputTruncation {
    max_bytes: usize,
}

impl ToolOutputTruncation {
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    /// Never truncates.
    pub fn disabled() -> Self {
        Self {
            max_bytes: usize::MAX,
        }
    }

    /// The text to record for the model. Under the cap, the input unchanged.
    /// Over the cap, a head+tail preview with a marker noting the omitted bytes
    /// and the temp file holding the full output.
    pub fn apply(&self, label: &str, text: &str) -> String {
        let Some(preview) = Preview::of(text, self.max_bytes) else {
            return text.to_string();
        };
        let location = persist(label, text)
            .map(|path| format!("; full output at {}", path.display()))
            .unwrap_or_default();
        format!(
            "{}\n…[codel00p truncated {} bytes of `{label}` output{location}]…\n{}",
            preview.head, preview.omitted, preview.tail
        )
    }
}

struct Preview<'a> {
    head: &'a str,
    tail: &'a str,
    omitted: usize,
}

impl<'a> Preview<'a> {
    /// Splits `text` into head/tail slices when it exceeds `max_bytes`; `None`
    /// when it fits. ~60% of the budget goes to the head, ~40% to the tail, both
    /// clamped to UTF-8 char boundaries so multibyte characters are never split.
    fn of(text: &'a str, max_bytes: usize) -> Option<Self> {
        if text.len() <= max_bytes {
            return None;
        }
        let head_budget = max_bytes * 6 / 10;
        let tail_budget = max_bytes - head_budget;
        let head_end = floor_boundary(text, head_budget);
        let tail_start = ceil_boundary(text, text.len().saturating_sub(tail_budget));
        if tail_start <= head_end {
            // Tiny cap or overlap: keep only the head.
            return Some(Self {
                head: &text[..head_end],
                tail: "",
                omitted: text.len() - head_end,
            });
        }
        Some(Self {
            head: &text[..head_end],
            tail: &text[tail_start..],
            omitted: tail_start - head_end,
        })
    }
}

/// Largest char boundary `<= index`.
fn floor_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Smallest char boundary `>= index`.
fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// Writes the full output to a uniquely named temp file, returning its path.
/// Best-effort: `None` if the write fails (the preview is still useful).
fn persist(label: &str, text: &str) -> Option<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let safe: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let path = std::env::temp_dir().join(format!("codel00p-tool-{safe}-{nanos}.txt"));
    fs::write(&path, text).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_output_under_the_cap_unchanged() {
        let policy = ToolOutputTruncation::new(100);
        assert_eq!(policy.apply("read_file", "small output"), "small output");
    }

    #[test]
    fn disabled_never_truncates() {
        let big = "x".repeat(1_000_000);
        assert_eq!(ToolOutputTruncation::disabled().apply("t", &big), big);
    }

    #[test]
    fn truncates_with_head_tail_and_marker() {
        let text = format!("HEAD{}TAIL", "x".repeat(1000));
        let out = ToolOutputTruncation::new(40).apply("run_command", &text);

        assert!(out.len() < text.len());
        assert!(out.starts_with("HEAD"));
        assert!(out.ends_with("TAIL"));
        assert!(out.contains("codel00p truncated"));
        assert!(out.contains("`run_command`"));
        assert!(out.contains("full output at"));
    }

    #[test]
    fn persisted_file_holds_the_full_output() {
        let text = format!("HEAD{}TAIL", "y".repeat(1000));
        let out = ToolOutputTruncation::new(40).apply("read_file", &text);

        let marker = out.split("full output at ").nth(1).expect("path in marker");
        let path = marker.split(']').next().unwrap().trim();
        let persisted = std::fs::read_to_string(path).expect("read persisted output");
        assert_eq!(persisted, text);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn preview_respects_utf8_char_boundaries() {
        // Multibyte chars; the cap lands mid-character but must not split one.
        let text = "é".repeat(200); // 2 bytes each → 400 bytes
        let preview = Preview::of(&text, 51).expect("over cap");
        assert!(text.starts_with(preview.head));
        assert!(text.ends_with(preview.tail));
        // Slices are valid UTF-8 (would panic on a bad boundary when formatting).
        let _ = format!("{}{}", preview.head, preview.tail);
    }
}
