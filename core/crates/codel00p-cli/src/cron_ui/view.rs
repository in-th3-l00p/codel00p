//! Pure rendering for the cron dialog. No state changes here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::model::{CreateStep, CronModel, Screen};
use crate::dialog::{accent, error, muted, panel, render_help, selection};
use crate::tui::picker::PickerItem;

/// The keybindings listed in the `?` help overlay, mirroring the live handlers.
const HELP: &[(&str, &str)] = &[
    ("↑/↓", "move selection"),
    ("type", "filter the job list"),
    ("↵", "open the selected job"),
    ("n", "new scheduled job"),
    ("e / d", "enable / disable (detail)"),
    ("R", "run the job now (detail)"),
    ("x", "delete the job — asks to confirm (detail)"),
    ("?", "toggle this help"),
    ("Esc", "back / quit"),
];

pub(crate) fn draw(frame: &mut Frame, model: &CronModel) {
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
        Paragraph::new(Line::from(Span::styled(" codel00p cron", accent()))),
        rows[0],
    );

    match model.screen {
        Screen::List => draw_list(frame, rows[1], model),
        Screen::Detail => draw_detail(frame, rows[1], model),
        Screen::Create => draw_create(frame, rows[1], model),
    }

    let hint = match model.screen {
        Screen::List => "↑/↓ move · type to filter · ↵ open · n new · ? help · Esc quit",
        Screen::Detail => {
            "e enable · d disable · R run now · x delete (confirms) · ? help · Esc back"
        }
        Screen::Create => "type · ↵ next · Esc cancel",
    };
    // A pending delete confirmation (or any status) replaces the hint; the confirm
    // prompt is error-styled so it stands out.
    let footer = match &model.status {
        Some(status) => {
            let style = if model.pending_delete.is_some() || model.status_is_error {
                error()
            } else {
                muted()
            };
            Span::styled(format!(" {status}"), style)
        }
        None => Span::styled(format!(" {hint}"), muted()),
    };
    frame.render_widget(Paragraph::new(Line::from(footer)), rows[2]);

    if model.show_help {
        render_help(frame, HELP);
    }
}

fn draw_list(frame: &mut Frame, area: Rect, model: &CronModel) {
    if model.picker.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  (no scheduled jobs — press n to add one)",
                muted(),
            ))
            .block(panel("jobs")),
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
    frame.render_widget(Paragraph::new(lines).block(panel("jobs")), area);
}

fn draw_detail(frame: &mut Frame, area: Rect, model: &CronModel) {
    let Some(row) = &model.selected else {
        frame.render_widget(Paragraph::new("").block(panel("job")), area);
        return;
    };
    let detail = &row.detail;
    let mut lines = vec![
        kv("id", &row.id),
        kv(
            "schedule",
            &format!("{} ({})", row.schedule, detail.schedule_spec),
        ),
        kv("enabled", if row.enabled { "yes" } else { "no" }),
    ];
    if let Some(workspace) = &detail.workspace {
        lines.push(kv("workspace", workspace));
    }
    if let Some(provider) = &detail.provider {
        lines.push(kv("provider", provider));
    }
    if let Some(model_name) = &detail.model {
        lines.push(kv("model", model_name));
    }
    let last_run = match detail.last_run_epoch {
        Some(epoch) => format!("{epoch} (epoch seconds)"),
        None => "never".to_string(),
    };
    lines.push(kv("last run", &last_run));
    lines.push(Line::from(""));
    match &detail.command {
        Some(command) => {
            lines.push(Line::from(Span::styled("command", accent())));
            lines.push(Line::from(format!("  codel00p {}", command.join(" "))));
        }
        None => {
            lines.push(Line::from(Span::styled("prompt", accent())));
            lines.push(Line::from(format!("  {}", detail.prompt)));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(panel("job")), area);
}

fn draw_create(frame: &mut Frame, area: Rect, model: &CronModel) {
    let (title, hint) = match model.create_step {
        CreateStep::Schedule => (
            "schedule",
            "e.g. 30m, 2h, 1d, 1w (optionally prefixed with `every`)",
        ),
        CreateStep::Prompt => ("prompt", "what the agent should do on each run"),
    };
    let mut body = vec![Line::from(Span::styled(
        format!("Enter {title}:"),
        accent(),
    ))];
    if model.create_step == CreateStep::Prompt {
        body.push(Line::from(Span::styled(
            format!("  schedule: {}", model.create_schedule),
            muted(),
        )));
    }
    body.push(Line::from(""));
    body.push(Line::from(format!("  {}", model.composer.text())));
    body.push(Line::from(""));
    body.push(Line::from(Span::styled(format!("  {hint}"), muted())));
    frame.render_widget(Paragraph::new(body).block(panel(title)), area);
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
