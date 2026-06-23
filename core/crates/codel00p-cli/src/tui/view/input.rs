//! The composer (input box) renderer and its cursor/word-wrap math. The wrapping
//! here must match what the user sees so the cursor lands correctly.
//! `super::render` calls `draw_input` and `composer_rows`.

use super::*;

/// Character-wraps `text` (hard break at `width`), preserving explicit newlines.
/// Used for the composer, where the cursor math must match the displayed wrapping.
fn char_wrap_lines(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for logical in text.split('\n') {
        let chars: Vec<char> = logical.chars().collect();
        if chars.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut i = 0;
        while i < chars.len() {
            let end = (i + width).min(chars.len());
            out.push(chars[i..end].iter().collect());
            i = end;
        }
    }
    out
}

/// The (row, col) the composer cursor lands on under `char_wrap_lines`.
fn char_cursor_rowcol(text: &str, width: usize, cursor: usize) -> (u16, u16) {
    let width = width.max(1);
    let mut row: usize = 0;
    let mut remaining = cursor;
    for logical in text.split('\n') {
        let len = logical.chars().count();
        if remaining <= len {
            return ((row + remaining / width) as u16, (remaining % width) as u16);
        }
        row += if len == 0 { 1 } else { len.div_ceil(width) };
        remaining -= len + 1; // +1 for the consumed newline
    }
    (row as u16, 0)
}

/// Visual rows the composer needs, including a trailing row for a cursor parked at
/// the end of a full line.
pub(super) fn composer_rows(text: &str, cursor: usize, width: usize) -> usize {
    let rows = char_wrap_lines(text, width).len().max(1);
    let (cursor_row, _) = char_cursor_rowcol(text, width, cursor);
    rows.max(cursor_row as usize + 1)
}

pub(super) fn draw_input(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let bg = Style::default().bg(theme.input_bg);
    let prompt_style = bg.fg(theme.accent);
    let text_w = composer_text_width(area.width);
    let prompt_cols = PROMPT.chars().count() as u16;

    let fill = area.width as usize;

    // Empty composer: a dim placeholder. Each row is padded so `input_bg` fills the
    // full block width (not just the text).
    if app.composer.is_empty() && !app.overlay.is_open() {
        let hint = super::super::flavor::placeholder(&app.session_label());
        let line = pad_to_width(
            vec![
                Span::styled(PROMPT, prompt_style),
                Span::styled(hint, bg.fg(theme.muted)),
            ],
            fill,
            theme.input_bg,
        );
        frame.render_widget(Paragraph::new(line).style(bg), area);
        frame.set_cursor_position(Position::new(area.x + prompt_cols, area.y));
        return;
    }

    // Each wrapped row gets the prompt (first row) or matching indent, padded to the
    // full width so the background block is a solid bar.
    let rows = char_wrap_lines(app.composer.text(), text_w);
    let lines: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let lead = if i == 0 { PROMPT } else { "  " };
            pad_to_width(
                vec![Span::styled(lead, prompt_style), Span::styled(row, bg)],
                fill,
                theme.input_bg,
            )
        })
        .collect();

    // Keep the cursor row visible when the input is taller than the (capped) box.
    let (cursor_row, cursor_col) =
        char_cursor_rowcol(app.composer.text(), text_w, app.composer.cursor());
    let input_scroll = cursor_row.saturating_sub(area.height.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(lines).style(bg).scroll((input_scroll, 0)),
        area,
    );

    if !app.overlay.is_open() && area.height > 0 {
        let x = area.x + prompt_cols + cursor_col.min(area.width.saturating_sub(prompt_cols + 1));
        let y = area.y + cursor_row.saturating_sub(input_scroll);
        frame.set_cursor_position(Position::new(x, y));
    }
}
