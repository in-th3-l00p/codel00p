//! Color palette for the agent TUI. A small, btop-inspired dark theme; kept as a
//! struct so a future settings option can swap it without touching the views.

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub(crate) struct Theme {
    pub(crate) accent: Color,
    pub(crate) user: Color,
    pub(crate) assistant: Color,
    pub(crate) tool: Color,
    pub(crate) notice: Color,
    pub(crate) error: Color,
    pub(crate) muted: Color,
    pub(crate) panel_border: Color,
    pub(crate) overlay_border: Color,
    pub(crate) selection_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Rgb(0x7a, 0xa2, 0xf7),
            user: Color::Rgb(0x9e, 0xce, 0x6a),
            assistant: Color::Rgb(0xc0, 0xca, 0xf5),
            tool: Color::Rgb(0xe0, 0xaf, 0x68),
            notice: Color::Rgb(0x7d, 0xcf, 0xff),
            error: Color::Rgb(0xf7, 0x76, 0x8e),
            muted: Color::Rgb(0x56, 0x5f, 0x89),
            panel_border: Color::Rgb(0x3b, 0x42, 0x61),
            overlay_border: Color::Rgb(0x7a, 0xa2, 0xf7),
            selection_bg: Color::Rgb(0x28, 0x34, 0x57),
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
