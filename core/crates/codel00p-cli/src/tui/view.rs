//! Renders `App` to the terminal. Pure draw logic — no state changes — so it can be
//! exercised against a `ratatui::backend::TestBackend` without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap};

use super::app::App;
use super::conversation::{Block as ChatBlock, ToolState};
use super::overlay::{EntityBrowser, EntityTab, ModelPicker, Overlay, SessionSwitcher};
use super::picker::{Picker, PickerItem};
use super::theme::Theme;

const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

pub(crate) fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

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

fn draw_conversation(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let mut lines: Vec<Line> = Vec::new();
    for block in &app.conversation.blocks {
        match block {
            ChatBlock::User(text) => push_wrapped(&mut lines, "you  ", theme.user, text),
            ChatBlock::Assistant(text) => push_wrapped(&mut lines, "ai   ", theme.assistant, text),
            ChatBlock::Notice(text) => push_wrapped(&mut lines, "·    ", theme.notice, text),
            ChatBlock::Error(text) => push_wrapped(&mut lines, "err  ", theme.error, text),
            ChatBlock::Tool { name, state } => {
                let (glyph, suffix) = match state {
                    ToolState::Requested => ("⋯", String::new()),
                    ToolState::Running(Some(message)) => ("⋯", format!(" — {message}")),
                    ToolState::Running(None) => ("⋯", String::new()),
                    ToolState::Done => ("✓", String::new()),
                    ToolState::Failed(message) => ("✗", format!(" — {message}")),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{glyph}    "), Style::default().fg(theme.tool)),
                    Span::styled(name.clone(), Style::default().fg(theme.tool)),
                    Span::styled(suffix, theme.muted()),
                ]));
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type a message and press Enter. F1 for help.",
            theme.muted(),
        )));
    }

    let viewport = area.height.saturating_sub(2) as usize;
    let scroll = lines.len().saturating_sub(viewport) as u16;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.panel_border))
        .title(" conversation ");
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn push_wrapped(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    color: ratatui::style::Color,
    text: &str,
) {
    let mut first = true;
    for segment in text.split('\n') {
        let lead = if first {
            prefix.to_string()
        } else {
            "     ".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(
                lead,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(segment.to_string(), Style::default().fg(color)),
        ]));
        first = false;
    }
}

fn draw_input(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let cursor = if app.overlay.is_open() { "" } else { "█" };
    let content = format!("{}{}", app.input, cursor);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(" message · Enter sends · F1 help ");
    frame.render_widget(Paragraph::new(content).block(block), area);
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
        Line::from("  F1           this help"),
        Line::from("  F2  /model   switch model"),
        Line::from("  F3  /entities browse projects · agents · MCP · memory"),
        Line::from("  F4  /org      organization (read-only)"),
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
        EntityTab::Org => draw_org(app, frame, rows[1]),
    }
}

fn draw_org(app: &App, frame: &mut Frame, area: Rect) {
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
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Switching orgs requires re-auth (a later release).",
                app.theme.muted(),
            )));
        }
        (None, Some(error)) => lines.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(app.theme.error),
        ))),
        (None, None) => lines.push(Line::from(Span::styled("  Loading…", app.theme.muted()))),
    }
    frame.render_widget(Paragraph::new(lines), area);
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

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
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
        app.input = "hello world".to_string();
        let rendered = render_to_string(&app, 80, 20);
        assert!(rendered.contains("codel00p"));
        assert!(rendered.contains("hello world"));
    }

    #[test]
    fn renders_conversation_blocks() {
        let mut app = test_app();
        app.conversation.push_user("ping");
        app.conversation.append_token("pong");
        let rendered = render_to_string(&app, 80, 20);
        assert!(rendered.contains("ping"));
        assert!(rendered.contains("pong"));
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = test_app();
        app.overlay = Overlay::Help;
        let rendered = render_to_string(&app, 80, 24);
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
        let rendered = render_to_string(&app, 120, 12);
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
        let rendered = render_to_string(&app, 120, 12);
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
        let rendered = render_to_string(&app, 80, 20);
        assert!(rendered.contains("switch session"));
        assert!(rendered.contains("chat-99"));
    }
}
