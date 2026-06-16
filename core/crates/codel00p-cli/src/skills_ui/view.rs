//! Pure rendering for the skills review dialog. No state changes here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};

use super::model::{Filter, Screen, SkillKind, SkillsModel};
use crate::dialog::{accent, muted, panel, selection};
use crate::tui::picker::PickerItem;

pub(crate) fn draw(frame: &mut Frame, model: &SkillsModel) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " codel00p skills review",
            accent(),
        ))),
        rows[0],
    );

    match model.screen {
        Screen::List => draw_list(frame, rows[1], model),
        Screen::Detail => draw_detail(frame, rows[1], model),
    }

    let footer = match model.screen {
        Screen::List => {
            "↑/↓ move · type to filter · Tab view · ↵ open · a approve · r reject · d disable · Esc quit"
        }
        Screen::Detail => detail_footer(model),
    };
    let footer = match &model.status {
        Some(status) => format!(" {status}"),
        None => format!(" {footer}"),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, muted()))),
        rows[2],
    );
}

fn detail_footer(model: &SkillsModel) -> &'static str {
    match model.selected.as_ref().map(|row| row.kind) {
        Some(SkillKind::Candidate) => "↑/↓ scroll · a approve · r reject · Esc back",
        Some(SkillKind::Active) => "↑/↓ scroll · d disable · Esc back",
        None => "↑/↓ scroll · Esc back",
    }
}

fn draw_list(frame: &mut Frame, area: Rect, model: &SkillsModel) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let titles: Vec<Line> = Filter::ORDER
        .iter()
        .map(|filter| Line::from(filter.label()))
        .collect();
    let active = Filter::ORDER
        .iter()
        .position(|filter| *filter == model.filter)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(active)
            .highlight_style(selection()),
        rows[0],
    );

    if model.picker.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("  (no skills in this view)", muted()))
                .block(panel("skills")),
            rows[1],
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
                selection()
            } else {
                Style::new()
            };
            Line::from(Span::styled(
                format!(
                    "{marker}{:<22} [{detail}]  {}",
                    row.name,
                    truncate(&row.description, 60)
                ),
                style,
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(panel("skills")), rows[1]);
}

fn draw_detail(frame: &mut Frame, area: Rect, model: &SkillsModel) {
    let Some(row) = &model.selected else {
        frame.render_widget(Paragraph::new("").block(panel("skill")), area);
        return;
    };
    let kind = match row.kind {
        SkillKind::Active => "active",
        SkillKind::Candidate => "candidate",
    };
    let mut lines = vec![
        kv("name", &row.name),
        kv("kind", kind),
        kv("source", &row.source),
    ];
    if let Some(version) = &row.version {
        lines.push(kv("version", version));
    }
    if let Some(by) = &row.created_by {
        lines.push(kv("by", by));
    }
    if row.kind == SkillKind::Active {
        let usage = if row.usage == 0 {
            "unused".to_string()
        } else {
            format!("used {}x", row.usage)
        };
        lines.push(kv("usage", &usage));
    }
    if !row.description.is_empty() {
        lines.push(kv("summary", &row.description));
    }
    if !row.triggers.is_empty() {
        lines.push(kv("triggers", &row.triggers.join(", ")));
    }
    lines.push(kv("path", &row.path));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("instructions", accent())));
    for line in row.body.lines() {
        lines.push(Line::from(format!("  {line}")));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((model.scroll as u16, 0))
            .block(panel(&row.name)),
        area,
    );
}

fn kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<10}"), accent()),
        Span::raw(value.to_string()),
    ])
}

fn truncate(text: &str, max: usize) -> String {
    let single_line = text.replace('\n', " ");
    if single_line.chars().count() <= max {
        single_line
    } else {
        let kept: String = single_line.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}
