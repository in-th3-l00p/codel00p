//! Renders `App` to the terminal. Pure draw logic — no state changes — so it can be
//! exercised against a `ratatui::backend::TestBackend` without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs};

use super::app::App;
use super::conversation::{Block as ChatBlock, ToolState};
use super::overlay::{EntityBrowser, EntityTab, ModelPicker, Overlay, SessionSwitcher};
use super::picker::{Picker, PickerItem};
use super::theme::Theme;

const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
/// The composer box grows with its content up to this many text rows, then scrolls.
const MAX_INPUT_ROWS: u16 = 6;
/// The role accent bar drawn down the left of a message block.
const BAR: &str = "▌";

pub(crate) fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    // Size the composer to its wrapped content (1..=MAX_INPUT_ROWS) so long input
    // grows and wraps instead of overflowing off the right edge.
    let input_inner_w = area.width.saturating_sub(2).max(1) as usize;
    let input_rows = composer_rows(app.composer.text(), app.composer.cursor(), input_inner_w)
        .clamp(1, MAX_INPUT_ROWS as usize) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(input_rows + 2),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(app, frame, chunks[0]);
    draw_conversation(app, frame, chunks[1]);
    draw_input(app, frame, chunks[2]);
    draw_status(app, frame, chunks[3]);

    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help(app, frame),
        Overlay::Permission(request) => draw_permission(app, frame, request),
        Overlay::Model(picker) => draw_model_picker(app, frame, picker),
        Overlay::Sessions(switcher) => draw_sessions(app, frame, switcher),
        Overlay::Entities(browser) => draw_entities(app, frame, browser),
    }
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![
        Span::styled("codel00p", app.theme.accent()),
        Span::styled("  agent", app.theme.muted()),
        Span::styled(
            format!("  ·  session {}", app.session_label()),
            app.theme.muted(),
        ),
    ];
    if let Some(version) = &app.update_available {
        spans.push(Span::styled(
            format!("   ⬆ v{version} available · run `codel00p update`"),
            Style::default()
                .fg(app.theme.notice)
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_conversation(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = app.theme.clone();
    let content_width = area.width.saturating_sub(2) as usize;
    let viewport_rows = area.height.saturating_sub(2);

    // Pre-wrap every block to the content width so each rendered `Line` is exactly
    // one visual row. The scroll math is then exact (the old bug counted logical
    // lines but rendered wrapped rows, so the newest content got clipped).
    let mut lines: Vec<Line> = Vec::new();
    for block in &app.conversation.blocks {
        lines.extend(block_lines(block, &theme, content_width));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type a message and press Enter. F1 for help.",
            theme.muted(),
        )));
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

    let title = if app.scroll.follow {
        " conversation ".to_string()
    } else {
        format!(
            " conversation · ↑{} · PgDn for latest ",
            app.scroll.offset_from_bottom
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(title);
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll_y, 0)),
        area,
    );
}

/// Renders one transcript block into styled, pre-wrapped rows. User and assistant
/// messages get a bold role header and a colored left accent bar; tools render as a
/// compact glyph line; notices/errors get a colored gutter glyph.
fn block_lines(block: &ChatBlock, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    match block {
        ChatBlock::User(text) => role_block(theme.user, "You", text, width),
        ChatBlock::Assistant(text) => role_block(theme.assistant, "codel00p", text, width),
        ChatBlock::Notice(text) => note_block(theme.notice, "·", text, width),
        ChatBlock::Error(text) => note_block(theme.error, "!", text, width),
        ChatBlock::Tool { name, state } => tool_lines(theme, name, state),
    }
}

/// A user/assistant message: a bold role label, then the wrapped body under a
/// colored accent bar, then a blank spacer.
fn role_block(color: Color, label: &str, text: &str, width: usize) -> Vec<Line<'static>> {
    let bar = Style::default().fg(color);
    let mut out = vec![Line::from(vec![
        Span::styled(format!("{BAR} "), bar),
        Span::styled(label.to_string(), bar.add_modifier(Modifier::BOLD)),
    ])];
    for row in wrap_text(text, width.saturating_sub(2).max(1)) {
        out.push(Line::from(vec![
            Span::styled(format!("{BAR} "), bar),
            Span::raw(row),
        ]));
    }
    out.push(Line::from(""));
    out
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
fn composer_rows(text: &str, cursor: usize, width: usize) -> usize {
    let rows = char_wrap_lines(text, width).len().max(1);
    let (cursor_row, _) = char_cursor_rowcol(text, width, cursor);
    rows.max(cursor_row as usize + 1)
}

fn draw_input(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let inner_w = area.width.saturating_sub(2).max(1) as usize;
    let lines: Vec<Line> = char_wrap_lines(app.composer.text(), inner_w)
        .into_iter()
        .map(Line::from)
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(" message · Enter ↵ send · Alt+Enter newline · F1 help ");
    let inner = block.inner(area);

    // Keep the cursor row visible when the input is taller than the (capped) box.
    let (cursor_row, cursor_col) =
        char_cursor_rowcol(app.composer.text(), inner_w, app.composer.cursor());
    let input_scroll = cursor_row.saturating_sub(inner.height.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((input_scroll, 0)),
        area,
    );

    if !app.overlay.is_open() && inner.height > 0 {
        let x = inner.x + cursor_col.min(inner.width.saturating_sub(1));
        let y = inner.y + cursor_row.saturating_sub(input_scroll);
        frame.set_cursor_position(Position::new(x, y));
    }
}

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let turn = if app.turn.running {
        let glyph = SPINNER[(app.tick as usize) % SPINNER.len()];
        match &app.turn.current_tool {
            Some(tool) => format!("{glyph} {tool}"),
            None => format!("{glyph} thinking"),
        }
    } else {
        "idle".to_string()
    };
    let org = app
        .cloud
        .viewer
        .as_ref()
        .and_then(|viewer| viewer.org())
        .map(|org| org.name().to_string())
        .unwrap_or_else(|| "no org".to_string());

    let line = Line::from(vec![
        Span::styled(format!(" {} ", app.options.provider), theme.selection()),
        Span::styled(
            format!(" {} ", app.options.model),
            Style::default().fg(theme.accent),
        ),
        Span::styled(format!("  {turn}"), Style::default().fg(theme.tool)),
        Span::styled(format!("   {}", usage_label(app)), theme.muted()),
        Span::styled(format!("   org: {org}"), theme.muted()),
        Span::styled(
            "    F2 model · F3 entities · F5 sessions · /help",
            theme.muted(),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// The status-bar usage meter: message count and an estimated-token total for the
/// current conversation. The token figure is an approximation (see
/// [`super::app::SessionUsage`]), so it is rendered with a leading `~`.
fn usage_label(app: &App) -> String {
    let usage = &app.usage;
    format!(
        "{} msg · ~{} tok",
        usage.messages,
        format_count(usage.estimated_tokens)
    )
}

/// Formats a token count compactly: `1234` stays as-is, larger values use a `k`
/// suffix (e.g. `12.3k`) so the meter fits the status bar.
fn format_count(tokens: u64) -> String {
    if tokens < 10_000 {
        tokens.to_string()
    } else {
        format!("{:.1}k", tokens as f64 / 1000.0)
    }
}

fn draw_help(app: &App, frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled("codel00p agent — keys", app.theme.accent())),
        Line::from(""),
        Line::from("  Enter        send the message"),
        Line::from("  Alt+Enter    newline in the composer"),
        Line::from("  ←/→ Home/End move/edit the cursor"),
        Line::from("  PgUp/PgDn    scroll the transcript · wheel scrolls too"),
        Line::from("  F1           this help"),
        Line::from("  F2  /model   switch model"),
        Line::from("  F3  /entities browse projects · agents · MCP · memory"),
        Line::from("  F4  /org      switch organization"),
        Line::from("  F5  /switch   resume a prior conversation"),
        Line::from("  /agents      jump to the agents tab"),
        Line::from("  /sessions /memory /history /tools /reset"),
        Line::from("  Esc          close overlay · clear input · quit"),
        Line::from("  Ctrl-C       quit"),
        Line::from(""),
        Line::from(Span::styled("  press any key to close", app.theme.muted())),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" help ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_permission(app: &App, frame: &mut Frame, request: &codel00p_harness::PermissionRequest) {
    let area = centered_rect(60, 30, frame.area());
    frame.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled("Permission requested", app.theme.accent())),
        Line::from(""),
        Line::from(format!("  tool:  {}", request.tool_name())),
        Line::from(format!("  scope: {:?}", request.scope())),
        Line::from(""),
        Line::from(Span::styled(
            "  [y] allow    [n] deny    [Esc] deny",
            app.theme.muted(),
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.error))
        .title(" approve tool ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_entities(app: &App, frame: &mut Frame, browser: &EntityBrowser) {
    let area = centered_rect(72, 72, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" organization ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let titles: Vec<Line> = EntityTab::ORDER
        .iter()
        .map(|tab| Line::from(tab.title()))
        .collect();
    let selected = EntityTab::ORDER
        .iter()
        .position(|tab| *tab == browser.tab)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .highlight_style(app.theme.selection())
            .style(app.theme.muted()),
        rows[0],
    );

    match browser.tab {
        EntityTab::Projects => {
            draw_picker(frame, rows[1], &app.theme, &browser.projects, "Projects")
        }
        EntityTab::Agents => draw_picker(
            frame,
            rows[1],
            &app.theme,
            &browser.agents,
            "Agents — Enter to use",
        ),
        EntityTab::Mcp => draw_picker(frame, rows[1], &app.theme, &browser.mcp, "MCP servers"),
        EntityTab::Memory => draw_picker(
            frame,
            rows[1],
            &app.theme,
            &browser.memory,
            "Approved memory",
        ),
        EntityTab::Users => draw_picker(frame, rows[1], &app.theme, &browser.users, "Users"),
        EntityTab::Org => draw_org(app, frame, rows[1], browser),
    }
}

fn draw_org(app: &App, frame: &mut Frame, area: Rect, browser: &EntityBrowser) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(1)])
        .split(area);
    let mut lines = vec![
        Line::from(Span::styled("Organization", app.theme.accent())),
        Line::from(""),
    ];
    match (&app.cloud.viewer, &app.cloud.error) {
        (Some(viewer), _) => {
            let org = viewer
                .org()
                .map(|org| org.name().to_string())
                .unwrap_or_else(|| "(personal)".to_string());
            let role = viewer
                .org_role()
                .map(|role| format!("{role:?}"))
                .unwrap_or_else(|| "—".to_string());
            lines.push(Line::from(format!("  org:   {org}")));
            lines.push(Line::from(format!("  role:  {role}")));
            if let Some(email) = viewer.email() {
                lines.push(Line::from(format!("  you:   {email}")));
            }
            lines.push(Line::from(
                "  Enter on an organization below to re-auth and switch.",
            ));
        }
        (None, Some(error)) => lines.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(app.theme.error),
        ))),
        (None, None) => lines.push(Line::from(Span::styled("  Loading…", app.theme.muted()))),
    }
    frame.render_widget(Paragraph::new(lines), rows[0]);
    draw_picker(
        frame,
        rows[1],
        &app.theme,
        &browser.orgs,
        "Organizations — Enter to switch",
    );
}

/// Draws the model picker: a `list_models` status line (loading / fell back to the
/// catalog) above the filterable model list. Selecting a row, or Enter on a typed id
/// the filter doesn't match, switches the model for the next turn.
fn draw_model_picker(app: &App, frame: &mut Frame, picker: &ModelPicker) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" switch model ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let status = picker
        .status
        .clone()
        .unwrap_or_else(|| "Enter to use · type any model id · Esc to close".to_string());
    frame.render_widget(
        Paragraph::new(Span::styled(format!("  {status}"), app.theme.muted())),
        rows[0],
    );
    draw_picker(frame, rows[1], &app.theme, &picker.picker, "Models");
}

/// Draws the session switcher: a status line above the list of prior conversations.
fn draw_sessions(app: &App, frame: &mut Frame, switcher: &SessionSwitcher) {
    let area = centered_rect(64, 60, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" switch session ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let status = switcher
        .status
        .clone()
        .unwrap_or_else(|| "Enter to resume · Esc to close".to_string());
    frame.render_widget(
        Paragraph::new(Span::styled(format!("  {status}"), app.theme.muted())),
        rows[0],
    );
    draw_picker(
        frame,
        rows[1],
        &app.theme,
        &switcher.sessions,
        "Prior conversations",
    );
}

fn draw_picker<T: PickerItem>(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    picker: &Picker<T>,
    title: &str,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let filter = if picker.query().is_empty() {
        "type to filter".to_string()
    } else {
        format!("filter: {}", picker.query())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{title}  "), theme.accent()),
            Span::styled(filter, theme.muted()),
        ])),
        rows[0],
    );

    if picker.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("  (nothing to show yet)", theme.muted())),
            rows[1],
        );
        return;
    }

    let items: Vec<ListItem> = picker
        .visible()
        .map(|(item, selected)| {
            let mut spans = vec![Span::styled(
                item.label(),
                if selected {
                    theme.selection()
                } else {
                    Style::default()
                },
            )];
            if let Some(detail) = item.detail() {
                spans.push(Span::styled(format!("  {detail}"), theme.muted()));
            }
            let prefix = if selected { "› " } else { "  " };
            ListItem::new(Line::from(
                std::iter::once(Span::styled(prefix, Style::default().fg(theme.accent)))
                    .chain(spans)
                    .collect::<Vec<_>>(),
            ))
        })
        .collect();
    frame.render_widget(List::new(items), rows[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::test_support::test_app;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| render(app, frame)).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn renders_header_and_input() {
        let mut app = test_app();
        app.composer.set_text("hello world");
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("codel00p"));
        assert!(rendered.contains("hello world"));
    }

    #[test]
    fn renders_conversation_blocks() {
        let mut app = test_app();
        app.conversation.push_user("ping");
        app.conversation.append_token("pong");
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("ping"));
        assert!(rendered.contains("pong"));
    }

    #[test]
    fn user_and_assistant_blocks_are_visually_labeled() {
        let mut app = test_app();
        app.conversation.push_user("hi there");
        app.conversation.finalize_assistant("hello back");
        let rendered = render_to_string(&mut app, 60, 20);
        // The "You" role header only comes from the user block.
        assert!(rendered.contains("You"));
        assert!(rendered.contains("hi there"));
        assert!(rendered.contains("hello back"));
        assert!(rendered.contains(BAR), "role accent bar should be drawn");
    }

    #[test]
    fn newest_message_is_visible_when_transcript_overflows() {
        // Regression: the old scroll math counted logical lines, not wrapped rows,
        // so the newest content was clipped below the viewport.
        let mut app = test_app();
        for i in 0..40 {
            app.conversation.push_user(format!(
                "a fairly long message number {i} to force wrapping"
            ));
        }
        app.conversation.finalize_assistant("NEWEST_VISIBLE_MARKER");
        let rendered = render_to_string(&mut app, 40, 12);
        assert!(
            rendered.contains("NEWEST_VISIBLE_MARKER"),
            "following mode must keep the newest line in view"
        );
    }

    #[test]
    fn scrolling_up_holds_older_content() {
        let mut app = test_app();
        for i in 0..40 {
            app.conversation.push_user(format!("OLD_LINE_{i}"));
        }
        app.conversation.finalize_assistant("BOTTOM");
        // Render once to populate the viewport height, then scroll to the top.
        render_to_string(&mut app, 40, 12);
        app.scroll.follow = false;
        app.scroll.offset_from_bottom = u16::MAX; // clamped to the top by the renderer
        let rendered = render_to_string(&mut app, 40, 12);
        assert!(
            rendered.contains("OLD_LINE_0"),
            "top of the scrollback is shown"
        );
        assert!(!app.scroll.follow);
    }

    #[test]
    fn long_input_wraps_and_stays_in_the_box() {
        let mut app = test_app();
        app.composer.set_text("WRAPME ".repeat(30));
        let rendered = render_to_string(&mut app, 30, 16);
        assert!(rendered.contains("WRAPME"));
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = test_app();
        app.overlay = Overlay::Help;
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("help"));
        assert!(rendered.contains("switch model"));
    }

    #[test]
    fn status_bar_renders_usage_meter() {
        let mut app = test_app();
        app.usage = crate::tui::app::SessionUsage {
            estimated_tokens: 1234,
            messages: 5,
        };
        let rendered = render_to_string(&mut app, 120, 12);
        assert!(rendered.contains("5 msg"));
        assert!(rendered.contains("~1234 tok"));
    }

    #[test]
    fn usage_meter_abbreviates_large_token_counts() {
        let mut app = test_app();
        app.usage = crate::tui::app::SessionUsage {
            estimated_tokens: 12_345,
            messages: 40,
        };
        let rendered = render_to_string(&mut app, 120, 12);
        assert!(rendered.contains("~12.3k tok"));
    }

    #[test]
    fn renders_session_switcher_overlay() {
        use crate::tui::overlay::{SessionSummary, SessionSwitcher};
        let mut switcher = SessionSwitcher::new();
        switcher.set_sessions(
            vec![SessionSummary {
                session_id: "chat-99".to_string(),
                source: "cli".to_string(),
                message_count: 2,
            }],
            None,
        );
        let mut app = test_app();
        app.overlay = Overlay::Sessions(switcher);
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("switch session"));
        assert!(rendered.contains("chat-99"));
    }
}
