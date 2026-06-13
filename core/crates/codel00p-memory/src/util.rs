//! Small normalization helpers shared by memory builders and parsers.

pub(crate) fn non_empty_filter(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
