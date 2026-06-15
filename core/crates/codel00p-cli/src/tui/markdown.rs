//! A small Markdown → ratatui renderer for assistant messages, inspired by the
//! Hermes chat TUI's markdown pane. Pure (text + theme + width → styled `Line`s),
//! so it is unit-tested directly. Each returned `Line` is pre-wrapped to one visual
//! row, matching how the transcript measures scroll.
//!
//! Supported: fenced code blocks (with a language label), ATX headings, horizontal
//! rules, blockquotes, unordered/ordered lists, and inline **bold**, *italic*, and
//! `code`. Unknown syntax falls through as plain text.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

/// Renders Markdown `text` into pre-wrapped, styled lines for the transcript.
pub(crate) fn render_markdown(text: &str, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let code_style = Style::default().fg(theme.tool);
    let raw: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut i = 0;

    while i < raw.len() {
        let line = raw[i];
        let trimmed = line.trim_start();

        // Fenced code block: ```lang … ```
        if let Some(rest) = trimmed.strip_prefix("```") {
            let lang = rest.trim();
            if !lang.is_empty() {
                out.push(Line::from(Span::styled(
                    format!("  ─ {lang}"),
                    Style::default().fg(theme.muted),
                )));
            }
            i += 1;
            while i < raw.len() && !raw[i].trim_start().starts_with("```") {
                for row in char_wrap(raw[i], width.saturating_sub(2)) {
                    out.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(row, code_style),
                    ]));
                }
                i += 1;
            }
            i += 1; // consume the closing fence
            continue;
        }

        // ATX heading: # … ######
        if let Some((level, content)) = heading(trimmed) {
            let prefix = "#".repeat(level);
            let mut segments = vec![(format!("{prefix} "), Style::default().fg(theme.accent))];
            segments.extend(inline_spans(
                content,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
                code_style,
            ));
            push_block(&mut out, "", Style::default(), segments, width);
            i += 1;
            continue;
        }

        // Horizontal rule.
        if is_hr(trimmed) {
            let rule = "─".repeat(width.min(40));
            out.push(Line::from(Span::styled(
                rule,
                Style::default().fg(theme.muted),
            )));
            i += 1;
            continue;
        }

        // Blockquote.
        if let Some(quote) = trimmed.strip_prefix('>') {
            let quote = quote.strip_prefix(' ').unwrap_or(quote);
            let segments = inline_spans(quote, Style::default().fg(theme.muted), code_style);
            push_block(
                &mut out,
                "│ ",
                Style::default().fg(theme.accent),
                segments,
                width,
            );
            i += 1;
            continue;
        }

        // Unordered list item.
        if let Some(item) = list_item(trimmed) {
            let segments = inline_spans(item, Style::default(), code_style);
            push_block(
                &mut out,
                "• ",
                Style::default().fg(theme.accent),
                segments,
                width,
            );
            i += 1;
            continue;
        }

        // Ordered list item.
        if let Some((marker, item)) = ordered_item(trimmed) {
            let segments = inline_spans(item, Style::default(), code_style);
            push_block(
                &mut out,
                &format!("{marker} "),
                Style::default().fg(theme.accent),
                segments,
                width,
            );
            i += 1;
            continue;
        }

        if trimmed.is_empty() {
            out.push(Line::from(""));
            i += 1;
            continue;
        }

        // Plain paragraph line.
        let segments = inline_spans(line, Style::default(), code_style);
        push_block(&mut out, "", Style::default(), segments, width);
        i += 1;
    }

    out
}

/// Wraps `segments` to `width` and pushes the rows, prefixing the first row with
/// `marker` (styled `marker_style`) and continuation rows with matching indent.
fn push_block(
    out: &mut Vec<Line<'static>>,
    marker: &str,
    marker_style: Style,
    segments: Vec<(String, Style)>,
    width: usize,
) {
    let indent = marker.chars().count();
    let rows = wrap_segments(segments, width.saturating_sub(indent).max(1));
    for (row_index, row) in rows.into_iter().enumerate() {
        let lead = if row_index == 0 {
            Span::styled(marker.to_string(), marker_style)
        } else {
            Span::raw(" ".repeat(indent))
        };
        let mut spans = vec![lead];
        spans.extend(row);
        out.push(Line::from(spans));
    }
}

/// Greedy word-wrap across styled segments, preserving each segment's style and
/// breaking words longer than a line by character.
fn wrap_segments(segments: Vec<(String, Style)>, width: usize) -> Vec<Vec<Span<'static>>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut current_width = 0usize;

    for (text, style) in segments {
        for word in text.split_inclusive(' ') {
            let word_width = word.chars().count();
            if current_width > 0 && current_width + word_width > width {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
            }
            if word_width > width {
                let mut chunk = String::new();
                for ch in word.chars() {
                    if current_width == width {
                        if !chunk.is_empty() {
                            current.push(Span::styled(std::mem::take(&mut chunk), style));
                        }
                        lines.push(std::mem::take(&mut current));
                        current_width = 0;
                    }
                    chunk.push(ch);
                    current_width += 1;
                }
                if !chunk.is_empty() {
                    current.push(Span::styled(chunk, style));
                }
            } else {
                current.push(Span::styled(word.to_string(), style));
                current_width += word_width;
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Splits an inline string into styled segments for `**bold**`, `*italic*`, and
/// `` `code` ``. Unmatched markers are treated as literal text.
fn inline_spans(text: &str, base: Style, code: Style) -> Vec<(String, Style)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<(String, Style)> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`'
            && let Some(close) = find_char(&chars, i + 1, '`')
        {
            flush(&mut buf, base, &mut out);
            out.push((chars[i + 1..close].iter().collect(), code));
            i = close + 1;
            continue;
        }
        if i + 1 < chars.len()
            && chars[i] == '*'
            && chars[i + 1] == '*'
            && let Some(close) = find_double_star(&chars, i + 2)
        {
            flush(&mut buf, base, &mut out);
            out.push((
                chars[i + 2..close].iter().collect(),
                base.add_modifier(Modifier::BOLD),
            ));
            i = close + 2;
            continue;
        }
        if chars[i] == '*'
            && let Some(close) = find_char(&chars, i + 1, '*')
        {
            flush(&mut buf, base, &mut out);
            out.push((
                chars[i + 1..close].iter().collect(),
                base.add_modifier(Modifier::ITALIC),
            ));
            i = close + 1;
            continue;
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush(&mut buf, base, &mut out);
    if out.is_empty() {
        out.push((String::new(), base));
    }
    out
}

fn flush(buf: &mut String, style: Style, out: &mut Vec<(String, Style)>) {
    if !buf.is_empty() {
        out.push((std::mem::take(buf), style));
    }
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

fn find_double_star(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Hard character-wrap (no word breaking), preserving nothing but the text — used
/// for code-block lines.
fn char_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    let mut rows = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let end = (i + width).min(chars.len());
        rows.push(chars[i..end].iter().collect());
        i = end;
    }
    rows
}

fn heading(trimmed: &str) -> Option<(usize, &str)> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &trimmed[hashes..];
        if let Some(content) = rest.strip_prefix(' ') {
            return Some((hashes, content));
        }
    }
    None
}

fn is_hr(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|c| c == '-')
            || compact.chars().all(|c| c == '*')
            || compact.chars().all(|c| c == '_'))
}

fn list_item(trimmed: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(item) = trimmed.strip_prefix(marker) {
            return Some(item);
        }
    }
    None
}

fn ordered_item(trimmed: &str) -> Option<(&str, &str)> {
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = &trimmed[digits..];
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    let item = rest.strip_prefix(' ')?;
    Some((&trimmed[..digits + 1], item))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn rendered(text: &str, width: usize) -> Vec<String> {
        let theme = Theme::default();
        render_markdown(text, &theme, width)
            .iter()
            .map(flat)
            .collect()
    }

    #[test]
    fn renders_bullets_and_headings() {
        let lines = rendered("# Title\n- one\n- two", 40);
        assert!(lines.iter().any(|l| l.contains("Title")));
        assert!(lines.iter().any(|l| l.starts_with("• one")));
        assert!(lines.iter().any(|l| l.starts_with("• two")));
    }

    #[test]
    fn fenced_code_block_keeps_lines_and_labels_language() {
        let lines = rendered("```rust\nlet x = 1;\n```", 40);
        assert!(lines.iter().any(|l| l.contains("─ rust")));
        assert!(lines.iter().any(|l| l.contains("let x = 1;")));
    }

    #[test]
    fn inline_bold_italic_and_code_become_separate_spans() {
        let theme = Theme::default();
        let lines = render_markdown("a **b** `c` *d*", &theme, 40);
        let spans = &lines[0].spans;
        // bold span
        assert!(
            spans
                .iter()
                .any(|s| s.content.as_ref() == "b" && s.style.add_modifier.contains(Modifier::BOLD))
        );
        // italic span
        assert!(
            spans
                .iter()
                .any(|s| s.content.as_ref() == "d"
                    && s.style.add_modifier.contains(Modifier::ITALIC))
        );
        // code span carries the tool color
        assert!(
            spans
                .iter()
                .any(|s| s.content.as_ref() == "c" && s.style.fg == Some(theme.tool))
        );
    }

    #[test]
    fn long_paragraph_wraps_to_width() {
        let lines = rendered(&"word ".repeat(20), 20);
        assert!(lines.len() > 1, "a long paragraph should wrap");
        assert!(lines.iter().all(|l| l.chars().count() <= 20));
    }

    #[test]
    fn blockquote_and_hr_render() {
        let lines = rendered("> quoted\n\n---", 40);
        assert!(lines.iter().any(|l| l.starts_with("│ quoted")));
        assert!(
            lines
                .iter()
                .any(|l| l.chars().all(|c| c == '─') && !l.is_empty())
        );
    }
}
