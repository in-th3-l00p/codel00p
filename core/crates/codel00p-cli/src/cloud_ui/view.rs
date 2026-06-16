//! Pure rendering for the `codel00p cloud` dialog.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::model::{CloudModel, DetailTab, Screen};
use crate::dialog::{accent, muted, panel, selection};
use crate::tui::picker::{Picker, PickerItem};

pub(crate) fn draw(frame: &mut Frame, model: &CloudModel) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(" codel00p cloud", accent()))),
        rows[0],
    );

    match model.screen {
        Screen::Status => draw_status(frame, rows[1], model),
        Screen::Detail => draw_detail(frame, rows[1], model),
        Screen::Unauthenticated => draw_unauthenticated(frame, rows[1], model),
    }

    let footer = match model.screen {
        Screen::Status => "↑/↓ move · ↵ open · p push · l pull · ? help · Esc quit",
        Screen::Detail => "Tab/←/→ switch · ↑/↓ move · type to filter · ? help · Esc back",
        Screen::Unauthenticated => "? help · Esc quit",
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {footer}"), muted()))),
        rows[2],
    );

    if model.show_help {
        draw_help(frame, model);
    }
}

fn draw_status(frame: &mut Frame, area: Rect, model: &CloudModel) {
    // Status panel grows with the viewer summary, an active-project line, and a
    // transient action line; the rest is the project list.
    let status_height =
        (model.viewer_lines.len() as u16 + 2) + 1 + if model.status.is_some() { 1 } else { 0 };
    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(status_height), Constraint::Min(1)])
        .split(area);

    let mut lines: Vec<Line> = model
        .viewer_lines
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect();
    let active = match &model.active_project {
        Some(project) => Line::from(vec![
            Span::styled("active project: ", muted()),
            Span::styled(project.name.clone(), accent()),
        ]),
        None => Line::from(Span::styled(
            "active project: (none — ↵ to select one)",
            muted(),
        )),
    };
    lines.push(active);
    if let Some(status) = &model.status {
        lines.push(Line::from(Span::styled(status.clone(), accent())));
    }
    frame.render_widget(Paragraph::new(lines).block(panel("status")), panes[0]);

    draw_picker(
        frame,
        panes[1],
        "projects",
        "(no projects)",
        &model.projects,
    );
}

fn draw_detail(frame: &mut Frame, area: Rect, model: &CloudModel) {
    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    // Tab strip, highlighting the active tab.
    let mut spans = Vec::new();
    for (index, tab) in DetailTab::ORDER.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if *tab == model.tab {
            accent().add_modifier(Modifier::UNDERLINED)
        } else {
            muted()
        };
        spans.push(Span::styled(tab.title().to_string(), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), panes[0]);

    let project = model
        .selected_project
        .as_ref()
        .map(|row| row.name.clone())
        .unwrap_or_else(|| "project".to_string());
    let empty = match model.tab {
        DetailTab::Agents => "(no agents)",
        DetailTab::Mcp => "(no MCP servers)",
        DetailTab::Memory => "(no memory)",
    };
    let title = format!("{project} · {}", model.tab.title());
    draw_picker(frame, panes[1], &title, empty, model.active_tab_picker());
}

fn draw_picker<T: PickerItem>(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    empty: &str,
    picker: &Picker<T>,
) {
    if picker.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(format!("  {empty}"), muted())).block(panel(title)),
            area,
        );
        return;
    }
    let lines: Vec<Line> = picker
        .visible()
        .map(|(row, is_selected)| {
            let marker = if is_selected { "▸ " } else { "  " };
            let style = if is_selected {
                selection()
            } else {
                Style::new()
            };
            let text = match row.detail() {
                Some(detail) => format!("{marker}{}  [{detail}]", row.label()),
                None => format!("{marker}{}", row.label()),
            };
            Line::from(Span::styled(text, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(panel(title)), area);
}

/// Renders the centered `?` help overlay, listing this dialog's keys for the
/// current screen. Drawn over everything (via [`Clear`]); any key dismisses it.
fn draw_help(frame: &mut Frame, model: &CloudModel) {
    let bindings: &[(&str, &str)] = match model.screen {
        Screen::Status => &[
            ("↑/↓", "move through projects"),
            ("type", "filter projects"),
            ("↵", "open project (sets the active push/pull target)"),
            ("p", "push approved local memory to the active project"),
            ("l", "pull approved cloud memory from the active project"),
            ("?", "toggle this help"),
            ("Esc", "quit"),
        ],
        Screen::Detail => &[
            ("Tab/←/→", "switch agents / MCP / memory"),
            ("↑/↓", "move through the list"),
            ("type", "filter the list"),
            ("?", "toggle this help"),
            ("Esc", "back to projects"),
        ],
        Screen::Unauthenticated => &[("?", "toggle this help"), ("Esc", "quit")],
    };

    let lines: Vec<Line> = bindings
        .iter()
        .map(|(keys, description)| {
            Line::from(vec![
                Span::styled(format!("  {keys:<9}"), accent()),
                Span::styled(format!("— {description}"), muted()),
            ])
        })
        .collect();

    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(panel("help")), area);
}

/// A centered rectangle covering `percent_x` × `percent_y` of `area`.
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

fn draw_unauthenticated(frame: &mut Frame, area: Rect, model: &CloudModel) {
    let mut lines: Vec<Line> = model
        .viewer_lines
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Run `codel00p auth login` to sign in.",
        accent(),
    )));
    frame.render_widget(Paragraph::new(lines).block(panel("not signed in")), area);
}
