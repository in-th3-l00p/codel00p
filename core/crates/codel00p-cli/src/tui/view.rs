//! Renders `App` to the terminal. Pure draw logic — no state changes — so it can be
//! exercised against a `ratatui::backend::TestBackend` without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs};

use super::app::App;
use super::conversation::{Block as ChatBlock, ToolState};
use super::overlay::{
    AdvancedKind, AdvancedPref, AdvancedSettingsOverlay, EntityBrowser, EntityTab, ModelPicker,
    Overlay, SessionSwitcher, SettingsOverlay, SettingsPref, SettingsRow, UpdatePrompt,
};
use super::picker::{Picker, PickerItem};
use super::theme::Theme;

const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
/// The composer box grows with its content up to this many text rows, then scrolls.
const MAX_INPUT_ROWS: u16 = 6;
/// The composer prompt marker.
const PROMPT: &str = "› ";

pub(crate) fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    // Size the composer (a borderless filled block) to its wrapped content, so long
    // input grows and wraps instead of overflowing off the right edge.
    let input_inner_w = composer_text_width(area.width);
    let input_rows = composer_rows(app.composer.text(), app.composer.cursor(), input_inner_w)
        .clamp(1, MAX_INPUT_ROWS as usize) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),          // header
            Constraint::Min(3),             // transcript (transparent)
            Constraint::Length(1),          // spacer
            Constraint::Length(input_rows), // composer (filled background)
            Constraint::Length(1),          // status
        ])
        .split(area);

    draw_header(app, frame, chunks[0]);
    draw_conversation(app, frame, chunks[1]);
    draw_input(app, frame, chunks[3]);
    draw_status(app, frame, chunks[4]);

    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help(app, frame),
        Overlay::Permission(request) => draw_permission(app, frame, request),
        Overlay::Model(picker) => draw_model_picker(app, frame, picker),
        Overlay::Sessions(switcher) => draw_sessions(app, frame, switcher),
        Overlay::Entities(browser) => draw_entities(app, frame, browser),
        Overlay::Command(palette) => draw_command(app, frame, palette),
        Overlay::Settings(settings) => draw_settings(app, frame, settings),
        Overlay::AdvancedSettings(advanced) => draw_advanced_settings(app, frame, advanced),
        Overlay::UpdatePrompt(prompt) => draw_update_prompt(app, frame, prompt),
    }
}

/// Width available for composer text after the `›` prompt and a right margin.
fn composer_text_width(area_width: u16) -> usize {
    area_width.saturating_sub(3).max(1) as usize
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
    let mut out = super::markdown::render_markdown(text, theme, width);
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
fn pad_to_width(mut spans: Vec<Span<'static>>, width: usize, bg: Color) -> Line<'static> {
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
    let bg = Style::default().bg(theme.input_bg);
    let prompt_style = bg.fg(theme.accent);
    let text_w = composer_text_width(area.width);
    let prompt_cols = PROMPT.chars().count() as u16;

    let fill = area.width as usize;

    // Empty composer: a dim placeholder. Each row is padded so `input_bg` fills the
    // full block width (not just the text).
    if app.composer.is_empty() && !app.overlay.is_open() {
        let hint = super::flavor::placeholder(&app.session_label());
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

fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let turn = turn_label(app);
    let org = app
        .cloud
        .viewer
        .as_ref()
        .and_then(|viewer| viewer.org())
        .map(|org| org.name().to_string())
        .unwrap_or_else(|| "no org".to_string());

    // The status bar layers two kinds of information. The progress HUD (spinner +
    // current tool + step N/max while running, a calm idle bar otherwise) is
    // ALWAYS shown — it is progress, not "advanced model info". The advanced bar,
    // gated on `show_advanced`, additionally shows provider + model, real token
    // usage, a context-used/size meter, and (when priced) the running cost.
    let mut spans = Vec::new();
    if app.show_advanced {
        spans.push(Span::styled(
            format!(" {} ", app.options.provider),
            theme.selection(),
        ));
        spans.push(Span::styled(
            format!(" {} ", app.options.model),
            Style::default().fg(theme.accent),
        ));
    }
    spans.push(Span::styled(
        format!("  {turn}"),
        Style::default().fg(theme.tool),
    ));
    // The latest verify-before-done / self-critique / failure-budget verdict, when
    // one has fired this session. Always shown (it is a trust signal, not model
    // internals); colored by outcome.
    if let Some(verification) = &app.verification {
        let style = if verification.starts_with('⚠') {
            Style::default().fg(theme.error)
        } else {
            Style::default().fg(theme.accent)
        };
        spans.push(Span::styled(format!("   {verification}"), style));
    }
    if app.show_advanced {
        spans.push(Span::styled(
            format!("   {}", usage_label(app)),
            theme.muted(),
        ));
        // The running cost rides next to the token meter, but only when a provider
        // actually priced the calls — never a bogus `$0.00` for free/local models.
        if let Some(cost) = cost_label(app) {
            spans.push(Span::styled(format!("  {cost}"), theme.muted()));
        }
        spans.push(Span::styled(
            format!("   {}", context_label(app)),
            theme.muted(),
        ));
    }
    spans.push(Span::styled(format!("   org: {org}"), theme.muted()));
    spans.push(Span::styled("    Ctrl+P menu · Enter send", theme.muted()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The always-on progress HUD text. While a turn is running it shows a spinner,
/// the current tool with its present-progressive verb (or a thinking verb between
/// tools), the loop's `step N/max` position, and a long-run charm — e.g.
/// `⠋ committing git_commit · step 3/25 · still working…`. When idle it returns a
/// calm `idle` bar. This is shown regardless of `show_advanced`.
fn turn_label(app: &App) -> String {
    if !app.turn.running {
        return "idle".to_string();
    }
    let glyph = SPINNER[(app.tick as usize) % SPINNER.len()];
    // `iterations` is the count of completed steps; the live step is the next one,
    // clamped to the ceiling so we never render `26/25`.
    let step = app
        .turn
        .iterations
        .saturating_add(1)
        .min(app.max_iterations);
    let progress = format!("step {step}/{}", app.max_iterations);
    let activity = match &app.turn.current_tool {
        Some(tool) => format!("{} {tool}", super::flavor::tool_verb(tool)),
        None => format!("{}…", super::flavor::thinking_verb(app.tick)),
    };
    // A reassuring charm once the turn has been running a while; omitted early on.
    let elapsed = app.tick.saturating_sub(app.turn.started_tick);
    match super::flavor::charm(app.tick, elapsed) {
        Some(charm) => format!("{glyph} {activity} · {progress} · {charm}"),
        None => format!("{glyph} {activity} · {progress}"),
    }
}

/// The running-cost HUD: a compact `$0.0021`-style figure from the latest
/// provider-reported [`CostEstimate`]. Returns `None` when no cost has been
/// reported (free/local models) or the reported total is zero, so the meter is
/// omitted rather than showing a misleading `$0.00`.
fn cost_label(app: &App) -> Option<String> {
    let cost = app.last_cost.as_ref()?;
    if cost.total_nanos == 0 {
        return None;
    }
    // Costs are nano-units of the currency (1e9 nanos == 1 unit). USD renders with
    // a `$`; any other currency falls back to a suffix so the figure is unambiguous.
    let value = cost.total_nanos as f64 / 1_000_000_000.0;
    let formatted = if value < 0.01 {
        format!("{value:.4}")
    } else {
        format!("{value:.2}")
    };
    match cost.currency.to_ascii_uppercase().as_str() {
        "USD" | "" => Some(format!("${formatted}")),
        other => Some(format!("{formatted} {other}")),
    }
}

/// The status-bar usage meter: message count and a token total for the current
/// conversation. Prefers the real provider total (no `~`) when an inference has
/// reported usage; otherwise falls back to the char-count estimate (with a
/// leading `~`; see [`super::app::SessionUsage`]).
fn usage_label(app: &App) -> String {
    match &app.last_usage {
        Some(usage) => format!(
            "{} msg · {} tok",
            app.usage.messages,
            format_count(usage.total_tokens())
        ),
        None => format!(
            "{} msg · ~{} tok",
            app.usage.messages,
            format_count(app.usage.estimated_tokens)
        ),
    }
}

/// The context meter: how much of the model's context window is in use. Context
/// used is the latest request's prompt-side tokens (input + cache), which is the
/// closest proxy for "tokens currently in context"; it falls back to the
/// char-count estimate before any usage arrives. The window size comes from the
/// static [`super::app::context_window`] table — rendered as `ctx 12.3k/200k
/// (6%)` when known, or `ctx 12.3k` when the window is unknown.
fn context_label(app: &App) -> String {
    let used = app
        .last_usage
        .as_ref()
        .map(|usage| usage.prompt_tokens())
        .unwrap_or(app.usage.estimated_tokens);
    match super::app::context_window(&app.options.provider, &app.options.model) {
        Some(window) if window > 0 => {
            let percent = ((used as f64 / window as f64) * 100.0).round() as u64;
            format!(
                "ctx {}/{} ({}%)",
                format_count(used),
                format_count(window as u64),
                percent
            )
        }
        _ => format!("ctx {}", format_count(used)),
    }
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
        Line::from(Span::styled(
            "  Ctrl+P       command menu — every action in one place",
            app.theme.accent(),
        )),
        Line::from("  Enter        send the message"),
        Line::from("  Alt+Enter    newline in the composer"),
        Line::from("  ←/→ Home/End move/edit the cursor"),
        Line::from("  PgUp/PgDn    scroll the transcript · wheel scrolls too"),
        Line::from("  F1           this help"),
        Line::from("  F2/F3/F5     model · organization · sessions (also in Ctrl+P)"),
        Line::from("  F2 (in sessions)  rename the highlighted conversation"),
        Line::from("  /sessions /memory /history /tools /reset"),
        Line::from("  Ctrl+P → Settings  advanced status info · update checks"),
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

    // In rename mode the status line becomes an inline title editor; otherwise it
    // shows the usual hint (now including the rename key).
    if let Some(rename) = &switcher.rename {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  rename: ", app.theme.accent()),
                Span::styled(format!("{}▏", rename.input), Style::default()),
                Span::styled("  Enter to save · Esc to cancel", app.theme.muted()),
            ])),
            rows[0],
        );
    } else {
        let status = switcher
            .status
            .clone()
            .unwrap_or_else(|| "Enter to resume · F2 to rename · Esc to close".to_string());
        frame.render_widget(
            Paragraph::new(Span::styled(format!("  {status}"), app.theme.muted())),
            rows[0],
        );
    }
    draw_picker(
        frame,
        rows[1],
        &app.theme,
        &switcher.sessions,
        "Prior conversations",
    );
}

/// Draws the command palette: a filterable list of every CLI action.
fn draw_command(app: &App, frame: &mut Frame, palette: &super::overlay::CommandPalette) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" command palette ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  type to filter · Enter to run · Esc to close",
            app.theme.muted(),
        )),
        rows[0],
    );
    draw_picker(frame, rows[1], &app.theme, &palette.picker, "Commands");
}

fn draw_settings(app: &App, frame: &mut Frame, settings: &SettingsOverlay) {
    let area = centered_rect(50, 40, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" settings ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  ↑/↓ move · Enter/Space toggle · ←/→ cycle profile · Esc to close",
            app.theme.muted(),
        )),
        rows[0],
    );

    let selected = settings.selected;
    let items: Vec<ListItem> = SettingsRow::ORDER
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let is_selected = index == selected;
            let prefix = if is_selected { "› " } else { "  " };
            // Toggle rows render a checkbox; the profile row shows the active name
            // in `‹ name ›` arrows; the "Advanced…" row renders a chevron since it
            // opens a sub-overlay rather than toggling.
            let body = match row {
                SettingsRow::Pref(pref) => {
                    let on = match pref {
                        SettingsPref::ShowAdvanced => app.show_advanced,
                        SettingsPref::CheckUpdates => app.check_updates,
                    };
                    let checkbox = if on { "[x]" } else { "[ ]" };
                    format!("{checkbox} {}", pref.label())
                }
                SettingsRow::Profile => {
                    let active = app.active_profile.as_deref().unwrap_or("(none)");
                    format!("‹ {active} › {}", row.label())
                }
                SettingsRow::Advanced => format!("›   {}", row.label()),
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(app.theme.accent)),
                Span::styled(
                    body,
                    if is_selected {
                        app.theme.selection()
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!("  {}", row.hint()), app.theme.muted()),
            ]))
        })
        .collect();
    frame.render_widget(List::new(items), rows[1]);
}

/// Draws the Advanced settings sub-overlay: the harness-loop knobs (iteration
/// count + numeric/boolean internals). Numeric rows show their value inline with
/// `‹ N ›` arrows; boolean rows show a checkbox. A help line explains these
/// affect the agent loop.
fn draw_advanced_settings(app: &App, frame: &mut Frame, advanced: &AdvancedSettingsOverlay) {
    let area = centered_rect(56, 50, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.overlay_border))
        .title(" settings · advanced ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  ↑/↓ move · ←/→ or -/+ adjust · Enter/Space toggle · Esc back",
            app.theme.muted(),
        )),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  These tune the agent loop — change with care.",
            app.theme.muted(),
        )),
        rows[1],
    );

    let selected = advanced.selected;
    let items: Vec<ListItem> = AdvancedPref::ORDER
        .iter()
        .enumerate()
        .map(|(index, pref)| {
            let is_selected = index == selected;
            let prefix = if is_selected { "› " } else { "  " };
            let value = match pref.kind() {
                AdvancedKind::Number { .. } => {
                    let current = match pref {
                        AdvancedPref::MaxIterations => app.max_iterations,
                        AdvancedPref::VerifyIterations => app.verify_iterations,
                        AdvancedPref::FailureBudget => app.failure_budget,
                        _ => 0,
                    };
                    format!("‹ {current:>3} ›")
                }
                AdvancedKind::Bool => {
                    let on = match pref {
                        AdvancedPref::SelfKnowledge => app.self_knowledge,
                        AdvancedPref::SelfState => app.self_state,
                        AdvancedPref::BasePrompt => app.base_prompt,
                        AdvancedPref::AutoPlan => app.auto_plan,
                        _ => false,
                    };
                    if on {
                        "  [x]  ".to_string()
                    } else {
                        "  [ ]  ".to_string()
                    }
                }
            };
            // Left-pad the label to a fixed column so the values line up.
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(app.theme.accent)),
                Span::styled(
                    format!("{:<22}{value}", pref.label()),
                    if is_selected {
                        app.theme.selection()
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!("  {}", pref.hint()), app.theme.muted()),
            ]))
        })
        .collect();
    frame.render_widget(List::new(items), rows[2]);
}

/// Draws the update-prompt panel: the current → latest version and the two
/// choices (Update now / Dismiss). Mirrors the other centered overlays.
fn draw_update_prompt(app: &App, frame: &mut Frame, prompt: &UpdatePrompt) {
    let area = centered_rect(56, 30, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.notice))
        .title(" update available ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let lines = vec![
        Line::from(Span::styled(
            "A new codel00p is available",
            app.theme.accent(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  v"),
            Span::styled(prompt.current.clone(), app.theme.muted()),
            Span::raw("  →  v"),
            Span::styled(
                prompt.latest.clone(),
                Style::default()
                    .fg(app.theme.notice)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter] update now    [Esc] dismiss",
            app.theme.muted(),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).block(Block::default()), inner);
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
    fn user_and_assistant_messages_both_render() {
        let mut app = test_app();
        app.conversation.push_user("hi there");
        app.conversation.finalize_assistant("hello back");
        let rendered = render_to_string(&mut app, 60, 20);
        // The user message carries the `›` prompt marker; both texts appear.
        assert!(rendered.contains('›'));
        assert!(rendered.contains("hi there"));
        assert!(rendered.contains("hello back"));
    }

    #[test]
    fn user_message_has_a_background_tint() {
        let mut app = test_app();
        app.conversation.push_user("tinted");
        let user_bg = app.theme.user_bg;
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(&mut app, frame))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let tinted = buffer.content().iter().any(|cell| cell.bg == user_bg);
        assert!(tinted, "user message should have a background tint");
    }

    #[test]
    fn command_palette_renders_actions() {
        use crate::tui::overlay::{CommandPalette, Overlay};
        let mut app = test_app();
        app.overlay = Overlay::Command(CommandPalette::new());
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("command palette"));
        assert!(rendered.contains("Switch model"));
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
    fn assistant_messages_render_markdown() {
        let mut app = test_app();
        app.conversation
            .finalize_assistant("Here you go:\n\n- first\n- second");
        let rendered = render_to_string(&mut app, 60, 20);
        // The markdown bullet glyph appears (assistant body is rendered as markdown).
        assert!(rendered.contains("• first"));
        assert!(rendered.contains("• second"));
    }

    #[test]
    fn empty_transcript_shows_the_welcome_banner() {
        let mut app = test_app();
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("codel00p"));
        assert!(rendered.contains("terminal coding agent"));
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = test_app();
        app.overlay = Overlay::Help;
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("help"));
        assert!(rendered.contains("command menu"));
    }

    #[test]
    fn status_bar_renders_usage_meter_when_advanced() {
        let mut app = test_app();
        app.show_advanced = true;
        app.usage = crate::tui::app::SessionUsage {
            estimated_tokens: 1234,
            messages: 5,
        };
        let rendered = render_to_string(&mut app, 120, 12);
        assert!(rendered.contains("5 msg"));
        assert!(rendered.contains("~1234 tok"));
    }

    #[test]
    fn usage_meter_abbreviates_large_token_counts_when_advanced() {
        let mut app = test_app();
        app.show_advanced = true;
        app.usage = crate::tui::app::SessionUsage {
            estimated_tokens: 12_345,
            messages: 40,
        };
        let rendered = render_to_string(&mut app, 120, 12);
        assert!(rendered.contains("~12.3k tok"));
    }

    #[test]
    fn status_bar_hides_advanced_info_by_default() {
        let mut app = test_app(); // show_advanced = false
        app.usage = crate::tui::app::SessionUsage {
            estimated_tokens: 1234,
            messages: 5,
        };
        let rendered = render_to_string(&mut app, 120, 12);
        // The model name and token/context meters are absent on the minimal bar,
        // but the org chip stays.
        assert!(!rendered.contains("claude-opus-4-8"));
        assert!(!rendered.contains("tok"));
        assert!(!rendered.contains("ctx "));
        assert!(rendered.contains("org:"));
    }

    #[test]
    fn advanced_status_bar_shows_model_and_context_meter() {
        use codel00p_protocol::TokenUsage;
        let mut app = test_app();
        app.show_advanced = true;
        // A real usage figure drives both the token total and the context meter.
        app.last_usage = Some(TokenUsage {
            input_tokens: 12_300,
            output_tokens: 500,
            ..TokenUsage::default()
        });
        let rendered = render_to_string(&mut app, 200, 12);
        assert!(rendered.contains("claude-opus-4-8"));
        // claude-opus-4-8 has a known 200k window: ctx 12.3k/200.0k (...%).
        assert!(rendered.contains("ctx 12.3k/200.0k"));
    }

    #[test]
    fn advanced_context_meter_shows_used_only_for_unknown_window() {
        use codel00p_protocol::TokenUsage;
        let mut app = test_app();
        app.show_advanced = true;
        app.options.model = "mystery-model-9000".to_string();
        app.last_usage = Some(TokenUsage {
            input_tokens: 12_300,
            ..TokenUsage::default()
        });
        let rendered = render_to_string(&mut app, 160, 12);
        assert!(rendered.contains("ctx 12.3k"));
        // No window known, so no "/<size>" suffix.
        assert!(!rendered.contains("ctx 12.3k/"));
    }

    #[test]
    fn progress_hud_shows_tool_and_step_while_running_without_advanced() {
        // The progress HUD is always-on: even with advanced info OFF, a running
        // turn shows a spinner, the current tool, and the step N/max position.
        let mut app = test_app(); // show_advanced = false
        app.turn.running = true;
        app.turn.current_tool = Some("git_commit".to_string());
        app.turn.iterations = 2; // 2 done → live step 3
        app.max_iterations = 25;
        let rendered = render_to_string(&mut app, 120, 12);
        // git_commit renders with its present-progressive verb.
        assert!(rendered.contains("committing git_commit"));
        assert!(rendered.contains("step 3/25"));
        // Still minimal: no model name / token meter.
        assert!(!rendered.contains("claude-opus-4-8"));
        assert!(!rendered.contains("tok"));
    }

    #[test]
    fn progress_hud_shows_calm_idle_bar_when_not_running() {
        let mut app = test_app();
        app.turn.running = false;
        let rendered = render_to_string(&mut app, 120, 12);
        assert!(rendered.contains("idle"));
        assert!(!rendered.contains("step "));
    }

    #[test]
    fn progress_step_clamps_to_the_iteration_ceiling() {
        let mut app = test_app();
        app.turn.running = true;
        app.turn.iterations = 25;
        app.max_iterations = 25;
        let rendered = render_to_string(&mut app, 120, 12);
        // Never renders `26/25`.
        assert!(rendered.contains("step 25/25"));
        assert!(!rendered.contains("26/25"));
    }

    #[test]
    fn status_bar_shows_cost_when_present() {
        use codel00p_protocol::CostEstimate;
        let mut app = test_app();
        app.show_advanced = true;
        app.last_cost = Some(CostEstimate {
            currency: "USD".to_string(),
            total_nanos: 2_100_000, // $0.0021
        });
        let rendered = render_to_string(&mut app, 200, 12);
        assert!(rendered.contains("$0.0021"));
    }

    #[test]
    fn status_bar_omits_cost_when_absent_or_zero() {
        use codel00p_protocol::CostEstimate;
        // No cost reported: no `$` figure.
        let mut app = test_app();
        app.show_advanced = true;
        let rendered = render_to_string(&mut app, 200, 12);
        assert!(!rendered.contains('$'));
        // A zero cost (free/local model reporting 0) is also omitted, never $0.00.
        app.last_cost = Some(CostEstimate {
            currency: "USD".to_string(),
            total_nanos: 0,
        });
        let rendered = render_to_string(&mut app, 200, 12);
        assert!(!rendered.contains("$0.00"));
    }

    #[test]
    fn status_bar_shows_verification_verdict() {
        let mut app = test_app(); // advanced OFF — verdict is always-on
        app.verification = Some("✓ Verified: test pass".to_string());
        let rendered = render_to_string(&mut app, 200, 12);
        assert!(rendered.contains("Verified"));
    }

    #[test]
    fn renders_settings_overlay() {
        use crate::tui::overlay::{Overlay, SettingsOverlay};
        let mut app = test_app();
        app.overlay = Overlay::Settings(SettingsOverlay::new());
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("settings"));
        assert!(rendered.contains("Show advanced info"));
        // Default is off, so the checkbox is empty.
        assert!(rendered.contains("[ ] Show advanced info"));
    }

    #[test]
    fn settings_overlay_lists_check_updates() {
        use crate::tui::overlay::{Overlay, SettingsOverlay};
        let mut app = test_app();
        app.overlay = Overlay::Settings(SettingsOverlay::new());
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("Check for updates on start"));
        // Default is on, so the checkbox is checked.
        assert!(rendered.contains("[x] Check for updates on start"));
    }

    #[test]
    fn settings_overlay_shows_advanced_entry() {
        use crate::tui::overlay::{Overlay, SettingsOverlay};
        let mut app = test_app();
        app.overlay = Overlay::Settings(SettingsOverlay::new());
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("Advanced…"));
    }

    #[test]
    fn settings_overlay_lists_profile_switcher() {
        use crate::tui::overlay::{Overlay, SettingsOverlay};
        let mut app = test_app();
        app.active_profile = Some("careful".to_string());
        app.overlay = Overlay::Settings(SettingsOverlay::new());
        let rendered = render_to_string(&mut app, 90, 24);
        assert!(rendered.contains("Agent profile"));
        // The active profile is shown inline in the row.
        assert!(rendered.contains("careful"));
    }

    #[test]
    fn renders_advanced_settings_overlay_with_values() {
        use crate::tui::overlay::{AdvancedSettingsOverlay, Overlay};
        let mut app = test_app();
        app.max_iterations = 42;
        app.overlay = Overlay::AdvancedSettings(AdvancedSettingsOverlay::new());
        let rendered = render_to_string(&mut app, 90, 30);
        assert!(rendered.contains("advanced"));
        // Numeric rows render their current value inline.
        assert!(rendered.contains("Max iterations"));
        assert!(rendered.contains("42"));
        assert!(rendered.contains("Verify iterations"));
        assert!(rendered.contains("Failure budget"));
        // The loop-internal toggles moved here.
        assert!(rendered.contains("Self-knowledge"));
        assert!(rendered.contains("Auto-plan guidance"));
    }

    #[test]
    fn renders_update_prompt_overlay() {
        use crate::tui::overlay::{Overlay, UpdatePrompt};
        let mut app = test_app();
        app.overlay = Overlay::UpdatePrompt(UpdatePrompt {
            current: "0.8.0".to_string(),
            latest: "0.9.0".to_string(),
        });
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("update available"));
        assert!(rendered.contains("v0.8.0"));
        assert!(rendered.contains("v0.9.0"));
        assert!(rendered.contains("update now"));
    }

    #[test]
    fn renders_session_switcher_overlay() {
        use crate::tui::overlay::{SessionSummary, SessionSwitcher};
        let mut switcher = SessionSwitcher::new();
        switcher.set_sessions(
            vec![SessionSummary {
                session_id: "chat-99".to_string(),
                title: Some("Debug release packaging".to_string()),
                source: "cli".to_string(),
                message_count: 2,
            }],
            None,
        );
        let mut app = test_app();
        app.overlay = Overlay::Sessions(switcher);
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("switch session"));
        assert!(rendered.contains("Debug release packaging"));
        assert!(rendered.contains("chat-99"));
    }
}
