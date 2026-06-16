//! Color palette for the agent TUI. A small, btop-inspired dark theme; kept as a
//! struct so a future settings option can swap it without touching the views.

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
            accent: Color::Rgb(0x7a, 0xa2, 0xf7),
            tool: Color::Rgb(0xe0, 0xaf, 0x68),
            notice: Color::Rgb(0x7d, 0xcf, 0xff),
            error: Color::Rgb(0xf7, 0x76, 0x8e),
            muted: Color::Rgb(0x56, 0x5f, 0x89),
            overlay_border: Color::Rgb(0x7a, 0xa2, 0xf7),
            selection_bg: Color::Rgb(0x28, 0x34, 0x57),
            input_bg: Color::Rgb(0x1f, 0x23, 0x35),
            user_bg: Color::Rgb(0x1a, 0x1d, 0x2b),
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
