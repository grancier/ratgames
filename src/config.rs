//! Central configuration — the in-code "header" of defaults.
//!
//! Nothing downstream hardcodes a dimension, colour, or size; it all flows from
//! here. `Default` supplies the current values, and the plain data layout is
//! ready to gain `#[derive(Deserialize)]` for file-driven config later.

use std::path::PathBuf;

use crate::color::{palette, Color};
use crate::geometry::{Point, Rect, Size};
use crate::text::TextColors;

/// The whole app's tunables in one tree.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub window: WindowConfig,
    pub screen: ScreenConfig,
    pub marquee: MarqueeConfig,
    pub input: InputConfig,
}

/// Physical window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub target_fps: usize,
    pub resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "ratgames".to_string(),
            width: 768,
            height: 768,
            target_fps: 60,
            resizable: true,
        }
    }
}

/// The low-resolution virtual screen the pixel world composes into.
#[derive(Debug, Clone, Copy)]
pub struct ScreenConfig {
    pub size: Size,
    pub backdrop: Color,
    pub letterbox: Color,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        Self {
            size: Size::new(256, 256),
            backdrop: palette::BG,
            letterbox: palette::LETTERBOX,
        }
    }
}

/// The scrolling big-text banner.
#[derive(Debug, Clone, Copy)]
pub struct MarqueeConfig {
    pub text_scale: u32,
    pub tracking: u32,
    pub shadow_depth: u32,
    pub gap: u32,
    pub speed: u32,
    pub colors: TextColors,
}

impl Default for MarqueeConfig {
    fn default() -> Self {
        Self {
            text_scale: 6,
            tracking: 1,
            shadow_depth: 3,
            gap: 14,
            speed: 2,
            colors: TextColors::default(),
        }
    }
}

/// The bottom input panel: a nested border framing an anti-aliased text line.
#[derive(Debug, Clone)]
pub struct InputConfig {
    /// Fraction of the window height the panel occupies (`0.0..=1.0`).
    pub height_fraction: f32,
    /// Outer margin from the panel edge to the first border, in device pixels.
    pub margin_px: u32,
    /// Inner padding from the innermost border to the text, in device pixels.
    pub padding_px: u32,
    /// Text caret width, in device pixels.
    pub caret_width_px: u32,
    pub background_color: Color,
    pub text_color: Color,
    pub border: BorderConfig,
    pub font: FontConfig,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            height_fraction: 0.15,
            margin_px: 8,
            padding_px: 8,
            caret_width_px: 2,
            background_color: Color::rgb(0x0A, 0x0A, 0x14),
            text_color: Color::rgb(0xF0, 0xF0, 0xF0),
            border: BorderConfig::default(),
            font: FontConfig::default(),
        }
    }
}

/// A nested (concentric) line border.
#[derive(Debug, Clone, Copy)]
pub struct BorderConfig {
    pub color: Color,
    /// Thickness of each line, in device pixels.
    pub line_thickness_px: u32,
    /// Number of concentric lines ("2 lines all around").
    pub line_count: u32,
    /// Gap between adjacent lines, in device pixels.
    pub line_gap_px: u32,
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            color: Color::rgb(0x87, 0xCE, 0xFA), // light blue
            line_thickness_px: 2,
            line_count: 2,
            line_gap_px: 3,
        }
    }
}

/// Font selection for the input overlay.
#[derive(Debug, Clone)]
pub struct FontConfig {
    /// On-screen size, in device pixels — never scaled with the pixel world.
    pub size_px: f32,
    pub source: FontSource,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            size_px: 20.0,
            source: FontSource::default(),
        }
    }
}

/// Where the input font comes from.
#[derive(Debug, Clone)]
pub enum FontSource {
    /// A monospace family resolved from the OS font database.
    System { family: String },
    /// A `.ttf`/`.ttc` at an explicit path.
    File { path: PathBuf },
    /// A font bundled into the binary (none is bundled yet).
    Embedded,
}

impl Default for FontSource {
    fn default() -> Self {
        FontSource::System {
            family: "Menlo".to_string(),
        }
    }
}

/// Resolved geometry of the input panel, all derived from [`InputConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputLayout {
    pub panel: Rect,
    /// Concentric border rects, outermost first.
    pub borders: Vec<Rect>,
    /// The rect glyphs are drawn (and clipped) into.
    pub text_area: Rect,
}

impl InputConfig {
    /// Compute the panel, border, and text rects for a given window size.
    /// Pure and literal-free — everything comes from `self`.
    #[must_use]
    pub fn layout(&self, window: Size) -> InputLayout {
        let panel_h = (f64::from(window.h) * f64::from(self.height_fraction)).round() as u32;
        let panel = Rect::new(
            Point::new(0, window.h.saturating_sub(panel_h) as i32),
            Size::new(window.w, panel_h),
        );

        let step = self.border.line_thickness_px + self.border.line_gap_px;
        let mut borders = Vec::with_capacity(self.border.line_count as usize);
        let mut inset = self.margin_px;
        for _ in 0..self.border.line_count {
            borders.push(inset_rect(panel, inset));
            inset += step;
        }

        // Text sits inside the innermost line: drop the trailing gap, add padding.
        let text_inset = inset.saturating_sub(self.border.line_gap_px) + self.padding_px;
        InputLayout {
            panel,
            borders,
            text_area: inset_rect(panel, text_inset),
        }
    }
}

/// Shrink a rect inward by `by` pixels on every side.
fn inset_rect(r: Rect, by: u32) -> Rect {
    let d = by as i32;
    Rect::new(
        Point::new(r.origin.x + d, r.origin.y + d),
        Size::new(r.size.w.saturating_sub(2 * by), r.size.h.saturating_sub(2 * by)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_sits_in_the_bottom_fraction() {
        let cfg = InputConfig::default();
        let l = cfg.layout(Size::new(1000, 1000));
        assert_eq!(l.panel.size, Size::new(1000, 150)); // 15%
        assert_eq!(l.panel.origin, Point::new(0, 850));
    }

    #[test]
    fn nested_borders_match_configured_count_and_nest_inward() {
        let cfg = InputConfig::default();
        let l = cfg.layout(Size::new(800, 600));
        assert_eq!(l.borders.len(), cfg.border.line_count as usize);
        // Each successive border is strictly inside the previous.
        for pair in l.borders.windows(2) {
            assert!(pair[1].origin.x > pair[0].origin.x);
            assert!(pair[1].size.w < pair[0].size.w);
        }
        // Text area is inside the innermost border.
        let inner = l.borders.last().unwrap();
        assert!(l.text_area.origin.x >= inner.origin.x);
    }

    #[test]
    fn defaults_hold_no_magic_numbers_downstream() {
        // The one place 20px / 0.15 / light-blue live is here.
        let cfg = InputConfig::default();
        assert!((cfg.font.size_px - 20.0).abs() < f32::EPSILON);
        assert!((cfg.height_fraction - 0.15).abs() < f32::EPSILON);
        assert_eq!(cfg.border.line_count, 2);
    }
}
