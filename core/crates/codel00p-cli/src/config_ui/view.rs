//! Pure rendering for the config dialog. No state changes happen here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::model::{ConfigModel, MENU_ITEMS, PERMISSION_MODES, ProvFocus, Screen, TOOL_SETS};
use crate::dialog::{accent, muted, panel, render_help, selection};

/// The keybindings listed in the `?` help overlay, mirroring the live handlers.
const HELP: &[(&str, &str)] = &[
    ("↑/↓", "move selection"),
    ("↵", "open section / select / accept"),
    ("Tab", "move between provider fields"),
    ("Space", "toggle a tool set"),
    ("s", "save (menu)"),
    ("q", "quit (menu)"),
    ("?", "toggle this help"),
    ("Esc", "back / quit"),
];

pub(crate) fn draw(frame: &mut Frame, model: &ConfigModel) {
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
            " codel00p configuration",
            accent(),
        ))),
        rows[0],
    );

    match model.screen {
        Screen::Menu => draw_menu(frame, rows[1], model),
        Screen::Providers => draw_providers(frame, rows[1], model),
        Screen::Tools => draw_tools(frame, rows[1], model),
        Screen::Permissions => draw_permissions(frame, rows[1], model),
    }

    let footer = match model.screen {
        Screen::Menu => "↑/↓ move · ↵ open · s save · q quit · ? help",
        Screen::Providers => "↑/↓ pick · ↵ select/accept · Tab fields · ? help · Esc back",
        Screen::Tools => "↑/↓ move · Space toggle · ? help · Esc back",
        Screen::Permissions => "↑/↓ move · ↵ choose · ? help · Esc back",
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {footer}"), muted()))),
        rows[2],
    );

    if model.show_help {
        render_help(frame, HELP);
    }
}

fn draw_menu(frame: &mut Frame, area: Rect, model: &ConfigModel) {
    let provider_summary = match (&model.provider, &model.model) {
        (Some(provider), Some(model)) => format!("{provider} · {model}"),
        (Some(provider), None) => provider.clone(),
        _ => "(not set)".to_string(),
    };
    let tools_summary = if model.tool_sets.is_empty() {
        "(default)".to_string()
    } else {
        model.tool_sets.join(", ")
    };
    let hints = [
        provider_summary,
        tools_summary,
        model.permission_mode.clone(),
        String::new(),
        String::new(),
    ];

    let mut lines = Vec::new();
    for (index, item) in MENU_ITEMS.iter().enumerate() {
        let style = if index == model.menu_cursor {
            selection()
        } else {
            Style::new()
        };
        let hint = &hints[index];
        let text = if hint.is_empty() {
            format!("  {item}")
        } else {
            format!("  {item:<14} {hint}")
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(Paragraph::new(lines).block(panel("settings")), area);
}

fn draw_providers(frame: &mut Frame, area: Rect, model: &ConfigModel) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Provider list.
    let mut list = Vec::new();
    for (index, row) in model.providers.iter().enumerate() {
        let is_cursor = index == model.prov_cursor && model.prov_focus == ProvFocus::List;
        let mark = if row.has_key { "x" } else { " " };
        let chosen = if model.provider.as_deref() == Some(row.id) {
            "▸"
        } else {
            " "
        };
        let style = if is_cursor { selection() } else { Style::new() };
        list.push(Line::from(Span::styled(
            format!("{chosen} [{mark}] {}", row.display_name),
            style,
        )));
    }
    frame.render_widget(Paragraph::new(list).block(panel("provider")), columns[0]);

    // Fields for the selected provider.
    let key_display = if model.key_input.is_empty() {
        "(unchanged)".to_string()
    } else {
        "*".repeat(model.key_input.chars().count())
    };
    let field = |label: &str, value: &str, focus: ProvFocus| {
        let style = if model.prov_focus == focus {
            selection()
        } else {
            Style::new()
        };
        Line::from(vec![
            Span::styled(format!("{label:<9}"), accent()),
            Span::styled(value.to_string(), style),
        ])
    };
    let provider_label = model.provider.as_deref().unwrap_or("(none selected)");
    let body = vec![
        Line::from(Span::styled(
            format!("Selected: {provider_label}"),
            accent(),
        )),
        Line::from(""),
        field("API key", &key_display, ProvFocus::Key),
        field("Model", &model.model_input, ProvFocus::Model),
        field("Base URL", &model.base_url_input, ProvFocus::BaseUrl),
        Line::from(""),
        Line::from(Span::styled(
            "Enter on a provider selects it; Tab moves between fields.",
            muted(),
        )),
    ];
    frame.render_widget(Paragraph::new(body).block(panel("details")), columns[1]);
}

fn draw_tools(frame: &mut Frame, area: Rect, model: &ConfigModel) {
    let mut lines = Vec::new();
    for (index, set) in TOOL_SETS.iter().enumerate() {
        let checked = model.tool_sets.iter().any(|s| s == set);
        let mark = if checked { "x" } else { " " };
        let style = if index == model.tools_cursor {
            selection()
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(format!("  [{mark}] {set}"), style)));
    }
    frame.render_widget(Paragraph::new(lines).block(panel("tool sets")), area);
}

fn draw_permissions(frame: &mut Frame, area: Rect, model: &ConfigModel) {
    let mut lines = Vec::new();
    for (index, mode) in PERMISSION_MODES.iter().enumerate() {
        let chosen = if *mode == model.permission_mode {
            "(o)"
        } else {
            "( )"
        };
        let style = if index == model.perms_cursor {
            selection()
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!("  {chosen} {mode}"),
            style,
        )));
    }
    frame.render_widget(Paragraph::new(lines).block(panel("permission mode")), area);
}
