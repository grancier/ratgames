//! The colour theme: named tokens that every component's default colours flow
//! from — the one place a re-skin happens.
//!
//! Coupling is deliberately *light*. Each component `Config`'s `Default` reads
//! its colours from [`Theme::default`], so changing a token restyles the
//! defaults in one place. A theme loaded from a file does **not** retro-propagate
//! into a colour a config set explicitly — the explicit value wins. (Runtime
//! re-theming — "change the theme, every component follows" — would need a
//! resolve step over `Option` colours; that is intentionally out of scope here.)
//!
//! The raw literals live in [`palette`] as `const`s;
//! `Theme` bundles them into a serialisable, per-instance value.

use crate::color::{Color, palette};

/// Named colour tokens. Roles, not hues, so a re-theme is a data change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Theme {
    /// Backdrop behind the pixel world.
    pub background: Color,
    /// Primary "on" colour — big-text letter bodies.
    pub fill: Color,
    /// Outlines and borders around glyphs.
    pub outline: Color,
    /// Extruded 3D drop-shadow for big text.
    pub shadow: Color,
    /// Bars around the letterboxed screen.
    pub letterbox: Color,
    /// UI accent — the input panel's nested border.
    pub accent: Color,
    /// Error / rejection — the reject cross.
    pub danger: Color,
    /// Alert — the "GAME OVER" sign.
    pub warning: Color,
    /// Foreground text on UI panels — the input line.
    pub ink: Color,
    /// UI panel background — behind the input line.
    pub panel: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: palette::BG,
            fill: palette::FILL,
            outline: palette::OUTLINE,
            shadow: palette::SHADOW,
            letterbox: palette::LETTERBOX,
            accent: palette::ACCENT,
            danger: palette::DANGER,
            warning: palette::WARNING,
            ink: palette::INK,
            panel: palette::PANEL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tokens_match_the_palette() {
        let t = Theme::default();
        assert_eq!(t.background, palette::BG);
        assert_eq!(t.accent, palette::ACCENT);
        assert_eq!(t.danger, palette::DANGER);
        assert_eq!(t.warning, palette::WARNING);
    }

    #[test]
    fn theme_round_trips_through_toml() {
        let t = Theme::default();
        let text = toml::to_string(&t).expect("serialize");
        assert_eq!(toml::from_str::<Theme>(&text).expect("parse"), t);
    }

    #[test]
    fn a_partial_theme_keeps_other_tokens_default() {
        let t: Theme = toml::from_str("accent = \"#FF0000\"").expect("parse");
        assert_eq!(t.accent, Color::rgb(0xFF, 0, 0));
        assert_eq!(t.background, palette::BG); // untouched
    }
}
