//! The status bar: the live HUD beneath the composer. In the default (calm)
//! layout it shows a progress/idle indicator; with advanced info on it adds the
//! model, token-usage and context meters, cost, and the latest verification
//! verdict. The `*_label` helpers format each field; `super::render` draws it.

use super::*;

pub(super) fn draw_status(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let turn = turn_label(app);
    let org = app
        .cloud
        .viewer
        .as_ref()
        .and_then(|viewer| viewer.org())
        .map(|org| org.name().to_string())
        .unwrap_or_else(|| "no org".to_string());

    // The status bar layers two kinds of information. The progress HUD (spinner +
    // current tool + step N/max while running, a calm idle bar otherwise) is
    // ALWAYS shown — it is progress, not "advanced model info". The advanced bar,
    // gated on `show_advanced`, additionally shows provider + model, real token
    // usage, a context-used/size meter, and (when priced) the running cost.
    let mut spans = Vec::new();
    if app.show_advanced {
        spans.push(Span::styled(
            format!(" {} ", app.options.provider),
            theme.selection(),
        ));
        spans.push(Span::styled(
            format!(" {} ", app.options.model),
            Style::default().fg(theme.accent),
        ));
    }
    spans.push(Span::styled(
        format!("  {turn}"),
        Style::default().fg(theme.tool),
    ));
    // The latest verify-before-done / self-critique / failure-budget verdict, when
    // one has fired this session. Always shown (it is a trust signal, not model
    // internals); colored by outcome.
    if let Some(verification) = &app.verification {
        let style = if verification.starts_with('⚠') {
            Style::default().fg(theme.error)
        } else {
            Style::default().fg(theme.accent)
        };
        spans.push(Span::styled(format!("   {verification}"), style));
    }
    if app.show_advanced {
        spans.push(Span::styled(
            format!("   {}", usage_label(app)),
            theme.muted(),
        ));
        // The running cost rides next to the token meter, but only when a provider
        // actually priced the calls — never a bogus `$0.00` for free/local models.
        if let Some(cost) = cost_label(app) {
            spans.push(Span::styled(format!("  {cost}"), theme.muted()));
        }
        spans.push(Span::styled(
            format!("   {}", context_label(app)),
            theme.muted(),
        ));
    }
    spans.push(Span::styled(format!("   org: {org}"), theme.muted()));
    spans.push(Span::styled("    Ctrl+P menu · Enter send", theme.muted()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The always-on progress HUD text. While a turn is running it shows a spinner,
/// the current tool with its present-progressive verb (or a thinking verb between
/// tools), the loop's `step N/max` position, and a long-run charm — e.g.
/// `⠋ committing git_commit · step 3/25 · still working…`. When idle it returns a
/// calm `idle` bar. This is shown regardless of `show_advanced`.
fn turn_label(app: &App) -> String {
    if !app.turn.running {
        return "idle".to_string();
    }
    let glyph = SPINNER[(app.tick as usize) % SPINNER.len()];
    // `iterations` is the count of completed steps; the live step is the next one,
    // clamped to the ceiling so we never render `26/25`.
    let step = app
        .turn
        .iterations
        .saturating_add(1)
        .min(app.max_iterations);
    let progress = format!("step {step}/{}", app.max_iterations);
    let activity = match &app.turn.current_tool {
        Some(tool) => format!("{} {tool}", super::super::flavor::tool_verb(tool)),
        None => format!("{}…", super::super::flavor::thinking_verb(app.tick)),
    };
    // A reassuring charm once the turn has been running a while; omitted early on.
    let elapsed = app.tick.saturating_sub(app.turn.started_tick);
    match super::super::flavor::charm(app.tick, elapsed) {
        Some(charm) => format!("{glyph} {activity} · {progress} · {charm}"),
        None => format!("{glyph} {activity} · {progress}"),
    }
}

/// The running-cost HUD: a compact `$0.0021`-style figure from the latest
/// provider-reported [`CostEstimate`]. Returns `None` when no cost has been
/// reported (free/local models) or the reported total is zero, so the meter is
/// omitted rather than showing a misleading `$0.00`.
fn cost_label(app: &App) -> Option<String> {
    let cost = app.last_cost.as_ref()?;
    if cost.total_nanos == 0 {
        return None;
    }
    // Costs are nano-units of the currency (1e9 nanos == 1 unit). USD renders with
    // a `$`; any other currency falls back to a suffix so the figure is unambiguous.
    let value = cost.total_nanos as f64 / 1_000_000_000.0;
    let formatted = if value < 0.01 {
        format!("{value:.4}")
    } else {
        format!("{value:.2}")
    };
    match cost.currency.to_ascii_uppercase().as_str() {
        "USD" | "" => Some(format!("${formatted}")),
        other => Some(format!("{formatted} {other}")),
    }
}

/// The status-bar usage meter: message count and a token total for the current
/// conversation. Prefers the real provider total (no `~`) when an inference has
/// reported usage; otherwise falls back to the char-count estimate (with a
/// leading `~`; see [`super::super::app::SessionUsage`]).
fn usage_label(app: &App) -> String {
    match &app.last_usage {
        Some(usage) => format!(
            "{} msg · {} tok",
            app.usage.messages,
            format_count(usage.total_tokens())
        ),
        None => format!(
            "{} msg · ~{} tok",
            app.usage.messages,
            format_count(app.usage.estimated_tokens)
        ),
    }
}

/// The context meter: how much of the model's context window is in use. Context
/// used is the latest request's prompt-side tokens (input + cache), which is the
/// closest proxy for "tokens currently in context"; it falls back to the
/// char-count estimate before any usage arrives. The window size comes from the
/// static [`super::super::app::context_window`] table — rendered as `ctx 12.3k/200k
/// (6%)` when known, or `ctx 12.3k` when the window is unknown.
fn context_label(app: &App) -> String {
    let used = app
        .last_usage
        .as_ref()
        .map(|usage| usage.prompt_tokens())
        .unwrap_or(app.usage.estimated_tokens);
    match super::super::app::context_window(&app.options.provider, &app.options.model) {
        Some(window) if window > 0 => {
            let percent = ((used as f64 / window as f64) * 100.0).round() as u64;
            format!(
                "ctx {}/{} ({}%)",
                format_count(used),
                format_count(window as u64),
                percent
            )
        }
        _ => format!("ctx {}", format_count(used)),
    }
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
