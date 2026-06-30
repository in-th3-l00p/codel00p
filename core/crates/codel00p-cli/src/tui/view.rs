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
    AdvancedKind, AdvancedPref, AdvancedSettingsOverlay, AgentCreateField, AgentCreateForm,
    AgentSwitcher, EntityBrowser, EntityTab, ModelPicker, Overlay, SessionSwitcher,
    SettingsOverlay, SettingsPref, SettingsRow, UpdatePrompt,
};
use super::picker::{Picker, PickerItem};
use super::theme::Theme;

mod conversation;
mod input;
mod overlays;
mod status;
use conversation::*;
use input::*;
use overlays::*;
use status::draw_status;

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
        Overlay::Menu(menu) => draw_menu(app, frame, menu),
        Overlay::Settings(settings) => draw_settings(app, frame, settings),
        Overlay::AdvancedSettings(advanced) => draw_advanced_settings(app, frame, advanced),
        Overlay::UpdatePrompt(prompt) => draw_update_prompt(app, frame, prompt),
        Overlay::AgentSwitcher(switcher) => draw_agent_switcher(app, frame, switcher),
        Overlay::AgentCreate(form) => draw_agent_create(app, frame, form),
        Overlay::AgentDetail(detail) => draw_agent_detail(app, frame, detail),
    }
}

/// Width available for composer text after the `›` prompt and a right margin.
pub(super) fn composer_text_width(area_width: u16) -> usize {
    area_width.saturating_sub(3).max(1) as usize
}

fn draw_header(app: &App, frame: &mut Frame, area: Rect) {
    // The active local agent (multi-agent personas, #13): the banner always names
    // which agent — and thus which memory — is live. `None` is the base/default
    // agent.
    let agent_label = app
        .active_agent
        .as_deref()
        .unwrap_or(super::agents::DEFAULT_AGENT_LABEL);
    let mut spans = vec![
        Span::styled("codel00p", app.theme.accent()),
        Span::styled(format!("  agent: {agent_label}"), app.theme.accent()),
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

pub(super) fn draw_picker<T: PickerItem>(
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

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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
    fn tool_lifecycle_glyphs_distinguish_requested_from_running() {
        // A queued tool reads as a hollow ○; once running it fills to ●. The
        // distinct glyphs let the lifecycle be read at a glance, not just by color.
        let mut app = test_app();
        app.conversation.tool_requested("grep");
        let requested = render_to_string(&mut app, 60, 12);
        assert!(
            requested.contains('○'),
            "requested tool shows a hollow glyph"
        );

        app.conversation.tool_progress("grep", None);
        let running = render_to_string(&mut app, 60, 12);
        assert!(running.contains('●'), "running tool fills the glyph");
    }

    #[test]
    fn overlay_hints_use_the_bare_key_grammar() {
        // The standardized hint grammar is bare keys (no brackets) joined by `·`.
        // The update prompt is the canary: it used to read `[Enter] update now`.
        use crate::tui::overlay::{Overlay, UpdatePrompt};
        let mut app = test_app();
        app.overlay = Overlay::UpdatePrompt(UpdatePrompt {
            current: "0.8.0".to_string(),
            latest: "0.9.0".to_string(),
        });
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("Enter to update now · Esc to dismiss"));
        assert!(!rendered.contains("[Enter]"), "no bracketed keys remain");
    }

    #[test]
    fn menu_renders_the_four_sections() {
        use crate::tui::overlay::{MainMenu, Overlay};
        let mut app = test_app();
        app.overlay = Overlay::Menu(MainMenu::new());
        let rendered = render_to_string(&mut app, 80, 24);
        assert!(rendered.contains("menu"));
        assert!(rendered.contains("Agent"));
        assert!(rendered.contains("Conversations"));
        assert!(rendered.contains("Organization"));
        assert!(rendered.contains("Settings"));
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
                description: None,
                source: "cli".to_string(),
                message_count: 2,
            }],
            None,
        );
        let mut app = test_app();
        app.overlay = Overlay::Sessions(switcher);
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("conversations"));
        // The always-present "new conversation" row and the prior conversation.
        assert!(rendered.contains("New conversation"));
        assert!(rendered.contains("Debug release packaging"));
        assert!(rendered.contains("chat-99"));
    }

    #[test]
    fn header_shows_default_agent_when_none_active() {
        let mut app = test_app();
        assert!(app.active_agent.is_none());
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("agent: default"));
    }

    #[test]
    fn header_shows_active_agent_name() {
        let mut app = test_app();
        app.active_agent = Some("scout".to_string());
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("agent: scout"));
    }

    #[test]
    fn renders_agent_switcher_overlay() {
        use crate::tui::overlay::{AgentChoice, AgentSwitcher};
        let mut switcher = AgentSwitcher::new();
        switcher.set_agents(
            vec![
                AgentChoice {
                    name: "default".to_string(),
                    description: None,
                    active: true,
                },
                AgentChoice {
                    name: "scout".to_string(),
                    description: Some("recon agent".to_string()),
                    active: false,
                },
            ],
            None,
        );
        let mut app = test_app();
        app.overlay = Overlay::AgentSwitcher(switcher);
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("agents"));
        // The always-present "new agent" row and a listed agent.
        assert!(rendered.contains("New agent"));
        assert!(rendered.contains("scout"));
        // The active agent is marked.
        assert!(rendered.contains("default ✓"));
    }

    #[test]
    fn renders_settings_with_new_rows() {
        let mut app = test_app();
        app.permission_mode = Some("ask".to_string());
        app.overlay = Overlay::Settings(crate::tui::overlay::SettingsOverlay::new());
        let rendered = render_to_string(&mut app, 90, 24);
        assert!(rendered.contains("Tool approvals"));
        assert!(rendered.contains("‹ ask ›"));
        assert!(rendered.contains("Provider API key"));
        assert!(rendered.contains("Account"));
    }

    #[test]
    fn renders_agent_detail_overlay() {
        use crate::tui::overlay::{AgentDetail, AgentDetailData};
        let mut detail = AgentDetail::loading("scout".to_string(), false);
        detail.apply(AgentDetailData {
            description: "recon agent".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-opus-4-8".to_string(),
            dispatch: "openai:gpt-4o".to_string(),
            persona: "You scout.".to_string(),
            memory_note: "memory.sqlite · 1.2 MB".to_string(),
        });
        let mut app = test_app();
        app.overlay = Overlay::AgentDetail(detail);
        let rendered = render_to_string(&mut app, 90, 24);
        assert!(rendered.contains("agent: scout"));
        assert!(rendered.contains("provider:"));
        assert!(rendered.contains("anthropic"));
        assert!(rendered.contains("dispatch:"));
        assert!(rendered.contains("memory.sqlite"));
    }

    #[test]
    fn renders_agent_create_form() {
        use crate::tui::overlay::AgentCreateForm;
        let mut form = AgentCreateForm::new();
        form.name = "scribe".to_string();
        let mut app = test_app();
        app.overlay = Overlay::AgentCreate(form);
        let rendered = render_to_string(&mut app, 80, 20);
        assert!(rendered.contains("create agent"));
        assert!(rendered.contains("scribe"));
        assert!(rendered.contains("name:"));
    }
}
