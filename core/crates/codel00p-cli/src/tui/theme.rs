//! Color palette for the agent TUI. The palette is a struct so the Settings
//! overlay can swap it (live + persisted via `tui.theme`) without touching views.
//!
//! Colors come from the **256-color palette** (`Color::Indexed`), not 24-bit RGB,
//! because common terminals (notably macOS Terminal.app) do not support truecolor
//! and render RGB escapes as wrong/garish colors. Indexed colors render correctly
//! across 256-color terminals and look consistent.

use ratatui::style::{Color, Modifier, Style};

/// A selectable named theme. `Dark` is the default; the others are chosen in the
/// Settings overlay and persisted to `tui.theme`. Adding a variant here (with an
/// entry in `ORDER` and a `palette()` arm) is all a new theme needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ThemeKind {
    #[default]
    Dark,
    Light,
    HighContrast,
    Solarized,
}

impl ThemeKind {
    /// All themes, in the order the Settings cycler steps through them.
    pub(crate) const ORDER: [ThemeKind; 4] = [
        ThemeKind::Dark,
        ThemeKind::Light,
        ThemeKind::HighContrast,
        ThemeKind::Solarized,
    ];

    /// The stable config token written to `tui.theme`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ThemeKind::Dark => "dark",
            ThemeKind::Light => "light",
            ThemeKind::HighContrast => "high-contrast",
            ThemeKind::Solarized => "solarized",
        }
    }

    /// The human label shown in the Settings overlay.
    pub(crate) fn label(self) -> &'static str {
        match self {
            ThemeKind::Dark => "Dark",
            ThemeKind::Light => "Light",
            ThemeKind::HighContrast => "High contrast",
            ThemeKind::Solarized => "Solarized",
        }
    }

    /// Parses a `tui.theme` token; unknown / unset tokens fall back to `Dark`.
    pub(crate) fn from_name(name: &str) -> Self {
        ThemeKind::ORDER
            .into_iter()
            .find(|kind| kind.name() == name.trim().to_ascii_lowercase())
            .unwrap_or(ThemeKind::Dark)
    }

    /// The next (or previous) theme in `ORDER`, wrapping — used by the cycler.
    pub(crate) fn cycle(self, forward: bool) -> Self {
        let order = ThemeKind::ORDER;
        let index = order.iter().position(|kind| *kind == self).unwrap_or(0);
        let next = if forward {
            (index + 1) % order.len()
        } else {
            (index + order.len() - 1) % order.len()
        };
        order[next]
    }

    /// The concrete color palette for this theme.
    fn palette(self) -> Theme {
        match self {
            // Soft, readable hues just elevated above a dark terminal.
            ThemeKind::Dark => Theme {
                kind: self,
                accent: Color::Indexed(111), // soft blue (#87afff)
                tool: Color::Indexed(179),   // gold (#d7af5f)
                notice: Color::Indexed(116), // light cyan (#87d7d7)
                error: Color::Indexed(210),  // soft red (#ff8787)
                muted: Color::Indexed(245),  // mid grey (#8a8a8a)
                overlay_border: Color::Indexed(111),
                selection_bg: Color::Indexed(238), // grey chip (#444444)
                input_bg: Color::Indexed(237),     // composer fill (#3a3a3a)
                user_bg: Color::Indexed(236),      // user message tint (#303030)
            },
            // Dark foregrounds + light fills, for a light terminal background.
            ThemeKind::Light => Theme {
                kind: self,
                accent: Color::Indexed(25), // deep blue (#005faf)
                tool: Color::Indexed(130),  // dark orange (#af5f00)
                notice: Color::Indexed(30), // teal (#008787)
                error: Color::Indexed(124), // dark red (#af0000)
                muted: Color::Indexed(244), // grey (#808080)
                overlay_border: Color::Indexed(25),
                selection_bg: Color::Indexed(252), // light grey (#d0d0d0)
                input_bg: Color::Indexed(254),     // (#e4e4e4)
                user_bg: Color::Indexed(255),      // (#eeeeee)
            },
            // Bright, bold accents on deep fills for maximum legibility.
            ThemeKind::HighContrast => Theme {
                kind: self,
                accent: Color::Indexed(15), // white (#ffffff)
                tool: Color::Indexed(11),   // bright yellow
                notice: Color::Indexed(14), // bright cyan
                error: Color::Indexed(9),   // bright red
                muted: Color::Indexed(250), // light grey (#bcbcbc)
                overlay_border: Color::Indexed(15),
                selection_bg: Color::Indexed(12), // bright blue chip
                input_bg: Color::Indexed(236),
                user_bg: Color::Indexed(235),
            },
            // Muted Solarized-inspired hues.
            ThemeKind::Solarized => Theme {
                kind: self,
                accent: Color::Indexed(32), // blue (#0087d7)
                tool: Color::Indexed(136),  // yellow (#af8700)
                notice: Color::Indexed(37), // cyan (#00afaf)
                error: Color::Indexed(160), // red (#d70000)
                muted: Color::Indexed(245),
                overlay_border: Color::Indexed(37),
                selection_bg: Color::Indexed(239),
                input_bg: Color::Indexed(238),
                user_bg: Color::Indexed(237),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Theme {
    /// Which named theme this palette came from (so the Settings cycler can show
    /// + advance from the active one).
    pub(crate) kind: ThemeKind,
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
        ThemeKind::Dark.palette()
    }
}

impl Theme {
    /// Builds the palette for a named theme.
    pub(crate) fn named(kind: ThemeKind) -> Self {
        kind.palette()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_round_trips_for_every_theme() {
        for kind in ThemeKind::ORDER {
            assert_eq!(ThemeKind::from_name(kind.name()), kind);
            // The built palette remembers which theme it came from.
            assert_eq!(Theme::named(kind).kind, kind);
        }
    }

    #[test]
    fn unknown_theme_name_falls_back_to_dark() {
        assert_eq!(ThemeKind::from_name("chartreuse"), ThemeKind::Dark);
        assert_eq!(ThemeKind::from_name(""), ThemeKind::Dark);
        // Case-insensitive.
        assert_eq!(ThemeKind::from_name("LIGHT"), ThemeKind::Light);
    }

    #[test]
    fn cycle_wraps_in_both_directions() {
        assert_eq!(ThemeKind::Dark.cycle(true), ThemeKind::Light);
        assert_eq!(ThemeKind::Dark.cycle(false), ThemeKind::Solarized);
        // A full forward cycle returns to the start.
        let mut kind = ThemeKind::Dark;
        for _ in 0..ThemeKind::ORDER.len() {
            kind = kind.cycle(true);
        }
        assert_eq!(kind, ThemeKind::Dark);
    }
}
