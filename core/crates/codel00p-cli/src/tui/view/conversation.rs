//! Transcript rendering: turning the conversation log into wrapped, themed lines
//! — user/assistant message blocks, notices, tool-call status, and the empty
//! welcome banner. `super::render` calls `draw_conversation`; `pad_to_width` (a
//! span-padding helper) is shared with the composer renderer in `super::input`.

use super::*;

pub(super) fn draw_conversation(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme.clone();
    // No border: the transcript is transparent. A 1-column left/right margin gives
    // the text and the user-message background blocks some breathing room.
    let inner = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };
    let content_width = inner.width as usize;
    let viewport_rows = inner.height;

    // Pre-wrap every block to the content width so each rendered `Line` is exactly
    // one visual row, keeping the scroll math exact.
    let mut lines: Vec<Line> = Vec::new();
    for block in &app.conversation.blocks {
        lines.extend(block_lines(block, &theme, content_width));
    }
    if lines.is_empty() {
        lines.extend(welcome_lines(&theme, content_width));
    }

    // The renderer is the source of truth for scroll: clamp the stored offset to the
    // real wrapped height and pin to the bottom while following.
    app.viewport_rows = viewport_rows;
    let total = lines.len() as u16;
    let max_offset = total.saturating_sub(viewport_rows);
    if app.scroll.follow {
        app.scroll.offset_from_bottom = 0;
    } else {
        app.scroll.offset_from_bottom = app.scroll.offset_from_bottom.min(max_offset);
    }
    let scroll_y = max_offset - app.scroll.offset_from_bottom;

    frame.render_widget(Paragraph::new(lines).scroll((scroll_y, 0)), inner);

    // A subtle "scrolled up" hint in the top-right when not following.
    if !app.scroll.follow {
        let hint = format!(" ↑{} · PgDn for latest ", app.scroll.offset_from_bottom);
        let hint_w = hint.chars().count() as u16;
        if hint_w < inner.width {
            let hint_area = Rect {
                x: inner.x + inner.width - hint_w,
                y: inner.y,
                width: hint_w,
                height: 1,
            };
            frame.render_widget(Paragraph::new(Span::styled(hint, theme.muted())), hint_area);
        }
    }
}

/// Renders one transcript block into styled, pre-wrapped rows. Text stays white;
/// user messages get a subtle full-width background to set them apart from the
/// transparent assistant output. Tools/notices render as compact colored lines.
fn block_lines(block: &ChatBlock, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    match block {
        ChatBlock::User(text) => user_block(theme, text, width),
        ChatBlock::Assistant(text) => assistant_block(theme, text, width),
        ChatBlock::Notice(text) => note_block(theme.notice, "·", text, width),
        ChatBlock::Error(text) => note_block(theme.error, "!", text, width),
        ChatBlock::Tool { name, state } => tool_lines(theme, name, state),
    }
}

/// The assistant message: the body rendered as Markdown (code blocks, lists,
/// headings, inline styles) on a transparent background, then a spacer.
fn assistant_block(theme: &Theme, text: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = super::super::markdown::render_markdown(text, theme, width);
    out.push(Line::from(""));
    out
}

/// The user message: white text on a subtle full-width background block, with a
/// dim `›` prompt marker, then a transparent spacer.
fn user_block(theme: &Theme, text: &str, width: usize) -> Vec<Line<'static>> {
    let bg = Style::default().bg(theme.user_bg);
    let prompt = Style::default().bg(theme.user_bg).fg(theme.muted);
    let mut out = Vec::new();
    for (i, row) in wrap_text(text, width.saturating_sub(2).max(1))
        .into_iter()
        .enumerate()
    {
        let lead = if i == 0 { "› " } else { "  " };
        let spans = vec![
            Span::styled(lead.to_string(), prompt),
            Span::styled(row, bg),
        ];
        out.push(pad_to_width(spans, width, theme.user_bg));
    }
    out.push(Line::from(""));
    out
}

/// Pads a line's spans with a trailing run of background-styled spaces so the row's
/// background fills the full content width (the transcript itself is transparent).
pub(super) fn pad_to_width(
    mut spans: Vec<Span<'static>>,
    width: usize,
    bg: Color,
) -> Line<'static> {
    let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    if used < width {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    Line::from(spans)
}

/// The empty-transcript welcome: a compact brand line and a tagline.
fn welcome_lines(theme: &Theme, _width: usize) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled("  ⌁ codel00p", theme.accent())),
        Line::from(Span::styled(
            "  your terminal coding agent — project memory that grows as you work",
            theme.muted(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Type a message and press Enter · Ctrl+P for the command menu",
            theme.muted(),
        )),
    ]
}

/// A notice/error line: a colored gutter glyph and wrapped text, then a spacer.
fn note_block(color: Color, glyph: &str, text: &str, width: usize) -> Vec<Line<'static>> {
    let style = Style::default().fg(color);
    let mut out: Vec<Line> = wrap_text(text, width.saturating_sub(2).max(1))
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let lead = if i == 0 {
                format!("{glyph} ")
            } else {
                "  ".to_string()
            };
            Line::from(vec![Span::styled(lead, style), Span::styled(row, style)])
        })
        .collect();
    out.push(Line::from(""));
    out
}

/// A compact tool-call line in the timeline, with a lifecycle glyph.
fn tool_lines(theme: &Theme, name: &str, state: &ToolState) -> Vec<Line<'static>> {
    let (glyph, color, suffix) = match state {
        ToolState::Requested => ("●", theme.muted, String::new()),
        ToolState::Running(Some(message)) => ("●", theme.tool, format!(" — {message}")),
        ToolState::Running(None) => ("●", theme.tool, String::new()),
        ToolState::Done => ("✓", theme.tool, String::new()),
        ToolState::Failed(message) => ("✗", theme.error, format!(" — {message}")),
    };
    vec![Line::from(vec![
        Span::styled(format!("  {glyph} "), Style::default().fg(color)),
        Span::styled(name.to_string(), theme.muted()),
        Span::styled(suffix, theme.muted()),
    ])]
}

/// Greedy word-wrap to `width` columns (char count), breaking words longer than a
/// line by character. Blank logical lines are preserved.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for logical in text.split('\n') {
        if logical.is_empty() {
            rows.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut line_w = 0usize;
        for word in logical.split_inclusive(' ') {
            let word_w = word.chars().count();
            if line_w > 0 && line_w + word_w > width {
                rows.push(std::mem::take(&mut line));
                line_w = 0;
            }
            if word_w > width {
                for ch in word.chars() {
                    if line_w == width {
                        rows.push(std::mem::take(&mut line));
                        line_w = 0;
                    }
                    line.push(ch);
                    line_w += 1;
                }
            } else {
                line.push_str(word);
                line_w += word_w;
            }
        }
        rows.push(line);
    }
    rows
}
