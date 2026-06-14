//! Renders `App` to the terminal. Pure draw logic — no state changes — so it can be
//! exercised against a `ratatui::backend::TestBackend` without a real terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap};

use super::app::App;
use super::conversation::{Block as ChatBlock, ToolState};
use super::overlay::{EntityBrowser, EntityTab, Overlay};
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
        Overlay::Model(picker) => {
            let area = centered_rect(60, 60, frame.area());
            frame.render_widget(Clear, area);
            draw_picker(frame, area, &app.theme, picker, "Switch model");
        }
        Overlay::Entities(browser) => draw_entities(app, frame, browser),
    }
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("codel00p", app.theme.accent()),
        Span::styled("  agent", app.theme.muted()),
        Span::styled(
            format!("  ·  session {}", app.session_label()),
            app.theme.muted(),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
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
        Span::styled(format!("   org: {org}"), theme.muted()),
        Span::styled("    F2 model · F3 entities · F4 org · /help", theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
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
        EntityTab::Users => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("Users", app.theme.accent())),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Member listing is pending a backend endpoint.",
                        app.theme.muted(),
                    )),
                ]),
                rows[1],
            );
        }
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
}
