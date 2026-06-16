//! Color palette for the agent TUI. A small, dark theme kept as a struct so a
//! future settings option can swap it without touching the views.
//!
//! Colors come from the **256-color palette** (`Color::Indexed`), not 24-bit RGB,
//! because common terminals (notably macOS Terminal.app) do not support truecolor
//! and render RGB escapes as wrong/garish colors. Indexed colors render correctly
//! across 256-color terminals and look consistent.

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub(crate) struct Theme {
    pub(crate) accent: Color,
    pub(crate) tool: Color,
    pub(crate) notice: Color,
    pub(crate) error: Color,
    pub(crate) muted: Color,
    pub(crate) overlay_border: Color,
    pub(crate) selection_bg: Color,
    /// Background fill behind the composer (the input is a filled block, no border).
    pub(crate) input_bg: Color,
    /// Subtle background behind user messages, to differentiate them from the
    /// transparent assistant output.
    pub(crate) user_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Foregrounds: soft, readable hues from the 256-palette.
            accent: Color::Indexed(111), // soft blue (#87afff)
            tool: Color::Indexed(179),   // gold (#d7af5f)
            notice: Color::Indexed(116), // light cyan (#87d7d7)
            error: Color::Indexed(210),  // soft red (#ff8787)
            muted: Color::Indexed(245),  // mid grey (#8a8a8a)
            overlay_border: Color::Indexed(111),
            // Backgrounds: subtle dark greys, just elevated above a dark terminal.
            selection_bg: Color::Indexed(238), // grey chip (#444444)
            input_bg: Color::Indexed(237),     // composer fill (#3a3a3a)
            user_bg: Color::Indexed(236),      // user message tint (#303030)
        }
    }
}

impl Theme {
    pub(crate) fn accent(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub(crate) fn muted(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub(crate) fn selection(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
}
