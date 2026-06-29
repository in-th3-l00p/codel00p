//! Overlay renderers: every modal/picker panel drawn over the main view — help,
//! permission prompt, entity browser, model picker, session switcher, agent
//! switcher/creator, the top-level menu, settings, and the update prompt.
//!
//! `super::render` dispatches to these by the active `Overlay`. They share the
//! `centered_rect` layout helper and the generic `draw_picker` from `super`.
//!
//! Key-hint convention (kept consistent across every overlay): bare key names
//! (no brackets), `·`-separated, in the form `<Key> to <verb>`. Esc reads `close`
//! for dismissable panels, `cancel` for forms that discard input, and `go back`
//! for a sub-overlay.

use super::*;

/// Frames an overlay: clears the region, draws a titled border in `color`, and
/// returns the inner content `Rect`. Centralizes the panel chrome every overlay
/// shared so framing stays identical across them.
fn framed(frame: &mut Frame, area: Rect, title: &str, color: Color) -> Rect {
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(title.to_string());
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    inner
}

pub(super) fn draw_help(app: &App, frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
    let inner = framed(frame, area, " help ", app.theme.overlay_border);
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
        Line::from("  F2/F3/F5     model · organization · conversations (also in Ctrl+P)"),
        Line::from("  in conversations  e edit (name + description) · d delete · Enter open"),
        Line::from("  /sessions /memory /history /tools /reset"),
        Line::from("  /agent /new-agent  switch · create a local agent (also in Ctrl+P)"),
        Line::from("  Ctrl+P → Settings  advanced status info · update checks"),
        Line::from("  Esc          close overlay · clear input · quit"),
        Line::from("  Ctrl-C       quit"),
        Line::from(""),
        Line::from(Span::styled("  Any key to close", app.theme.muted())),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_permission(
    app: &App,
    frame: &mut Frame,
    request: &codel00p_harness::PermissionRequest,
) {
    let area = centered_rect(60, 30, frame.area());
    let inner = framed(frame, area, " approve tool ", app.theme.error);
    let lines = vec![
        Line::from(Span::styled("Permission requested", app.theme.accent())),
        Line::from(""),
        Line::from(format!("  tool:  {}", request.tool_name())),
        Line::from(format!("  scope: {:?}", request.scope())),
        Line::from(""),
        Line::from(Span::styled(
            "  y to allow · n to deny · Esc to deny",
            app.theme.muted(),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_entities(app: &App, frame: &mut Frame, browser: &EntityBrowser) {
    let area = centered_rect(72, 72, frame.area());
    let inner = framed(frame, area, " organization ", app.theme.overlay_border);

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

pub(super) fn draw_org(app: &App, frame: &mut Frame, area: Rect, browser: &EntityBrowser) {
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
pub(super) fn draw_model_picker(app: &App, frame: &mut Frame, picker: &ModelPicker) {
    let area = centered_rect(60, 60, frame.area());
    let inner = framed(frame, area, " switch model ", app.theme.overlay_border);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let status = picker
        .status
        .clone()
        .unwrap_or_else(|| "Enter to use · type to filter · Esc to close".to_string());
    frame.render_widget(
        Paragraph::new(Span::styled(format!("  {status}"), app.theme.muted())),
        rows[0],
    );
    draw_picker(frame, rows[1], &app.theme, &picker.picker, "Models");
}

/// Draws the conversations overlay: a "＋ New conversation" row plus the prior
/// conversations, with inline name+description editing and a delete confirmation.
pub(super) fn draw_sessions(app: &App, frame: &mut Frame, switcher: &SessionSwitcher) {
    let area = centered_rect(64, 60, frame.area());
    let inner = framed(frame, area, " conversations ", app.theme.overlay_border);

    // Edit mode replaces the list with a two-field editor.
    if let Some(edit) = &switcher.edit {
        draw_session_edit(app, frame, inner, edit);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // The status line shows the delete confirmation, the loading/empty status, or
    // the usual key hints.
    if let Some(confirm) = &switcher.confirm_delete {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Delete ", Style::default().fg(app.theme.error)),
                Span::styled(format!("\"{}\"", confirm.label), app.theme.accent()),
                Span::styled(" ?  y to delete · n / Esc to cancel", app.theme.muted()),
            ])),
            rows[0],
        );
    } else {
        let status = switcher.status.clone().unwrap_or_else(|| {
            "↑/↓ to move · Enter to open · e edit · d delete · Esc to close".to_string()
        });
        frame.render_widget(
            Paragraph::new(Span::styled(format!("  {status}"), app.theme.muted())),
            rows[0],
        );
    }
    draw_picker(frame, rows[1], &app.theme, &switcher.rows, "Conversations");
}

/// Draws the inline conversation editor (name + description), with the focused
/// field highlighted — mirrors the create-agent form's two-field layout.
fn draw_session_edit(
    app: &App,
    frame: &mut Frame,
    inner: Rect,
    edit: &super::super::overlay::SessionEdit,
) {
    use super::super::overlay::SessionEditField;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint
            Constraint::Length(1), // spacer
            Constraint::Length(1), // name
            Constraint::Length(1), // description
            Constraint::Min(1),    // padding
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "  Tab to switch field · Enter to save · Esc to cancel",
            app.theme.muted(),
        )),
        rows[0],
    );

    let field_line = |label: &str, value: &str, focused: bool| {
        let marker = if focused { "› " } else { "  " };
        let cursor = if focused { "▏" } else { "" };
        Line::from(vec![
            Span::styled(marker, Style::default().fg(app.theme.accent)),
            Span::styled(format!("{label:<13}"), app.theme.muted()),
            Span::styled(
                format!("{value}{cursor}"),
                if focused {
                    app.theme.selection()
                } else {
                    Style::default()
                },
            ),
        ])
    };

    frame.render_widget(
        Paragraph::new(field_line(
            "name:",
            &edit.name,
            edit.field == SessionEditField::Name,
        )),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(field_line(
            "description:",
            &edit.description,
            edit.field == SessionEditField::Description,
        )),
        rows[3],
    );
}

/// Draws the agent overlay (multi-agent personas, #13): a "＋ New agent" row plus
/// the local agents (default + registry) with the active one marked, and a delete
/// confirmation prompt.
pub(super) fn draw_agent_switcher(app: &App, frame: &mut Frame, switcher: &AgentSwitcher) {
    let area = centered_rect(60, 60, frame.area());
    let inner = framed(frame, area, " agents ", app.theme.overlay_border);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    if let Some(confirm) = &switcher.confirm_delete {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Delete agent ", Style::default().fg(app.theme.error)),
                Span::styled(format!("“{}”", confirm.name), app.theme.accent()),
                Span::styled(" ?  y to delete · n / Esc to cancel", app.theme.muted()),
            ])),
            rows[0],
        );
    } else {
        let status = switcher.status.clone().unwrap_or_else(|| {
            "↑/↓ to move · Enter to use/create · d delete · Esc to close".to_string()
        });
        frame.render_widget(
            Paragraph::new(Span::styled(format!("  {status}"), app.theme.muted())),
            rows[0],
        );
    }
    draw_picker(frame, rows[1], &app.theme, &switcher.rows, "Agents");
}

/// Draws the create-agent form (multi-agent personas, #13): a required name and
/// an optional description, with the focused field highlighted.
pub(super) fn draw_agent_create(app: &App, frame: &mut Frame, form: &AgentCreateForm) {
    let area = centered_rect(56, 36, frame.area());
    let inner = framed(frame, area, " create agent ", app.theme.overlay_border);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint
            Constraint::Length(1), // spacer
            Constraint::Length(1), // name
            Constraint::Length(1), // description
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // error / help
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "  Tab to switch field · Enter to create + switch · Esc to cancel",
            app.theme.muted(),
        )),
        rows[0],
    );

    let field_line = |label: &str, value: &str, focused: bool| {
        let marker = if focused { "› " } else { "  " };
        let cursor = if focused { "▏" } else { "" };
        Line::from(vec![
            Span::styled(marker, Style::default().fg(app.theme.accent)),
            Span::styled(format!("{label:<13}"), app.theme.muted()),
            Span::styled(
                format!("{value}{cursor}"),
                if focused {
                    app.theme.selection()
                } else {
                    Style::default()
                },
            ),
        ])
    };

    frame.render_widget(
        Paragraph::new(field_line(
            "name:",
            &form.name,
            form.field == AgentCreateField::Name,
        )),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(field_line(
            "description:",
            &form.description,
            form.field == AgentCreateField::Description,
        )),
        rows[3],
    );

    let footer = match &form.error {
        Some(error) => Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(app.theme.error),
        )),
        None => Line::from(Span::styled(
            "  name: letters, digits, - _ . (no spaces or slashes)",
            app.theme.muted(),
        )),
    };
    frame.render_widget(Paragraph::new(footer), rows[5]);
}

/// Draws the top-level Ctrl+P menu: the four focused sections that replaced the
/// old flat action list.
pub(super) fn draw_menu(app: &App, frame: &mut Frame, menu: &super::super::overlay::MainMenu) {
    let area = centered_rect(50, 50, frame.area());
    let inner = framed(frame, area, " menu ", app.theme.overlay_border);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  ↑/↓ to move · Enter to open · type to filter · Esc to close",
            app.theme.muted(),
        )),
        rows[0],
    );
    draw_picker(frame, rows[1], &app.theme, &menu.picker, "Sections");
}

pub(super) fn draw_settings(app: &App, frame: &mut Frame, settings: &SettingsOverlay) {
    let area = centered_rect(50, 40, frame.area());
    let inner = framed(frame, area, " settings ", app.theme.overlay_border);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  ↑/↓ to move · Enter/Space to toggle · ←/→ to cycle profile · Esc to close",
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
pub(super) fn draw_advanced_settings(
    app: &App,
    frame: &mut Frame,
    advanced: &AdvancedSettingsOverlay,
) {
    let area = centered_rect(56, 50, frame.area());
    let inner = framed(
        frame,
        area,
        " settings · advanced ",
        app.theme.overlay_border,
    );

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
            "  ↑/↓ to move · ←/→ or -/+ to adjust · Enter/Space to toggle · Esc to go back",
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
pub(super) fn draw_update_prompt(app: &App, frame: &mut Frame, prompt: &UpdatePrompt) {
    let area = centered_rect(56, 30, frame.area());
    let inner = framed(frame, area, " update available ", app.theme.notice);

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
            "  Enter to update now · Esc to dismiss",
            app.theme.muted(),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
