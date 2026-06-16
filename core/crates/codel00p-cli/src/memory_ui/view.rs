//! Pure rendering for the memory review dialog. No state changes here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};

use super::model::{MemoryModel, PendingAction, Screen, StatusFilter, kind_label, status_label};
use crate::dialog::{accent, muted, panel, render_help, selection};
use crate::tui::picker::PickerItem;

/// The keybindings listed in the `?` help overlay, mirroring the live handlers.
const HELP: &[(&str, &str)] = &[
    ("↑/↓", "move selection"),
    ("type", "filter the list"),
    ("Tab", "cycle the status filter"),
    ("↵", "open the selected record"),
    ("a", "approve (detail)"),
    ("r", "reject — prompts for a reason (detail)"),
    ("x", "archive — prompts for a reason (detail)"),
    ("e", "edit the content (detail)"),
    ("m", "merge into another record (detail)"),
    ("u", "restore prior content (detail)"),
    ("?", "toggle this help"),
    ("Esc", "back / quit"),
];

pub(crate) fn draw(frame: &mut Frame, model: &MemoryModel) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " codel00p memory review",
            accent(),
        ))),
        rows[0],
    );

    match model.screen {
        Screen::List => draw_list(frame, rows[1], model),
        Screen::Detail => draw_detail(frame, rows[1], model),
        Screen::Prompt => draw_prompt(frame, rows[1], model),
        Screen::SelectMerge => draw_merge(frame, rows[1], model),
        Screen::SelectRestore => draw_restore(frame, rows[1], model),
    }

    let footer = match model.screen {
        Screen::List => "↑/↓ move · type to filter · Tab status · ↵ open · ? help · Esc quit",
        Screen::Detail => {
            "a approve · r reject · x archive · e edit · m merge · u restore · ? help · Esc back"
        }
        Screen::Prompt => "type · ↵ confirm · Esc cancel",
        Screen::SelectMerge => "↑/↓ move · type to filter · ↵ merge into · Esc cancel",
        Screen::SelectRestore => "↑/↓ move · ↵ restore · Esc cancel",
    };
    let footer = match &model.status {
        Some(status) => format!(" {status}"),
        None => format!(" {footer}"),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(footer, muted()))),
        rows[2],
    );

    if model.show_help {
        render_help(frame, HELP);
    }
}

fn draw_list(frame: &mut Frame, area: Rect, model: &MemoryModel) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let titles: Vec<Line> = StatusFilter::ORDER
        .iter()
        .map(|filter| Line::from(filter.label()))
        .collect();
    let active = StatusFilter::ORDER
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
            Paragraph::new(Span::styled("  (no memory in this view)", muted()))
                .block(panel("memory")),
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
                format!("{marker}{}  [{detail}]", truncate(&row.label(), 80)),
                style,
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(panel("memory")), rows[1]);
}

fn draw_detail(frame: &mut Frame, area: Rect, model: &MemoryModel) {
    let Some(row) = &model.selected else {
        frame.render_widget(Paragraph::new("").block(panel("memory")), area);
        return;
    };
    let mut lines = vec![
        kv("id", &row.id),
        kv("status", status_label(row.status)),
        kv("kind", kind_label(row.kind)),
        kv("tags", &row.tags.join(", ")),
        Line::from(""),
        Line::from(Span::styled("content", accent())),
        Line::from(format!("  {}", row.content)),
    ];
    if !model.detail_audit.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("audit", accent())));
        for event in &model.detail_audit {
            lines.push(Line::from(format!(
                "  {}\t{}\t{}\t{}",
                event.sequence,
                event.action,
                event.actor,
                event.reason.as_deref().unwrap_or("")
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(panel("record")), area);
}

fn draw_merge(frame: &mut Frame, area: Rect, model: &MemoryModel) {
    let header = model
        .selected
        .as_ref()
        .map(|row| format!("Merge {} into:", row.id))
        .unwrap_or_else(|| "Merge into:".to_string());
    let lines: Vec<Line> = std::iter::once(Line::from(Span::styled(header, accent())))
        .chain(std::iter::once(Line::from("")))
        .chain(model.merge_targets.visible().map(|(row, is_selected)| {
            let marker = if is_selected { "▸ " } else { "  " };
            let detail = row.detail().unwrap_or_default();
            let style = if is_selected {
                selection()
            } else {
                Style::new()
            };
            Line::from(Span::styled(
                format!("{marker}{}  [{detail}]", truncate(&row.label(), 80)),
                style,
            ))
        }))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(panel("merge target")), area);
}

fn draw_restore(frame: &mut Frame, area: Rect, model: &MemoryModel) {
    let lines: Vec<Line> = std::iter::once(Line::from(Span::styled(
        "Restore content from audit entry:",
        accent(),
    )))
    .chain(std::iter::once(Line::from("")))
    .chain(model.restore_picker.visible().map(|(row, is_selected)| {
        let marker = if is_selected { "▸ " } else { "  " };
        let detail = row.detail().unwrap_or_default();
        let style = if is_selected {
            selection()
        } else {
            Style::new()
        };
        Line::from(Span::styled(
            format!("{marker}[{detail}]  {}", truncate(&row.label(), 70)),
            style,
        ))
    }))
    .collect();
    frame.render_widget(Paragraph::new(lines).block(panel("restore")), area);
}

fn draw_prompt(frame: &mut Frame, area: Rect, model: &MemoryModel) {
    let title = match model.pending {
        Some(PendingAction::Reject) => "reason to reject",
        Some(PendingAction::Archive) => "reason to archive",
        Some(PendingAction::Edit) => "new content",
        None => "input",
    };
    let body = vec![
        Line::from(Span::styled(format!("Enter {title}:"), accent())),
        Line::from(""),
        Line::from(format!("  {}", model.composer.text())),
    ];
    frame.render_widget(Paragraph::new(body).block(panel(title)), area);
}

fn kv(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<8}"), accent()),
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
