//! Pure rendering for the sessions browser dialog.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::model::{Screen, SessionsModel};
use crate::tui::picker::PickerItem;

const ACCENT: Style = Style::new().add_modifier(Modifier::BOLD);

fn selected() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

fn muted() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

pub(crate) fn draw(frame: &mut Frame, model: &SessionsModel) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(" codel00p sessions", ACCENT))),
        rows[0],
    );

    match model.screen {
        Screen::List => draw_list(frame, rows[1], model),
        Screen::Detail => draw_detail(frame, rows[1], model),
    }

    let footer = match model.screen {
        Screen::List => "↑/↓ move · type to filter · ↵ open · Esc quit",
        Screen::Detail => "↑/↓ scroll · Esc back",
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {footer}"), muted()))),
        rows[2],
    );
}

fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
}

fn draw_list(frame: &mut Frame, area: Rect, model: &SessionsModel) {
    if model.picker.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("  (no sessions yet)", muted())).block(block("sessions")),
            area,
        );
        return;
    }
    let lines: Vec<Line> = model
        .picker
        .visible()
        .map(|(row, is_selected)| {
            let marker = if is_selected { "▸ " } else { "  " };
            let detail = row.detail().unwrap_or_default();
            let style = if is_selected {
                selected()
            } else {
                Style::new()
            };
            Line::from(Span::styled(
                format!("{marker}{}  [{detail}]", row.label()),
                style,
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block("sessions")), area);
}

fn draw_detail(frame: &mut Frame, area: Rect, model: &SessionsModel) {
    let title = model
        .selected
        .as_ref()
        .map(|row| row.id.clone())
        .unwrap_or_else(|| "transcript".to_string());
    let lines: Vec<Line> = model
        .transcript
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((model.scroll as u16, 0))
            .block(block(&title)),
        area,
    );
}
