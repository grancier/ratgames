//! Device-space drawing primitives for overlays: anti-aliased text runs.
//!
//! These draw into the window [`Surface`] *after* the integer upscale, in device
//! pixels, so glyphs are never pixel-scaled. This is the smooth-text counterpart
//! to the pixel-art [`text`](crate::text) path (which bakes 1-bit sprites for the
//! virtual screen). [`InputField`](crate::input::InputField) composes these;
//! other overlays — a score, a label, a menu item — can reuse them rather than
//! re-implementing glyph layout.

use crate::color::Color;
use crate::font::{LineMetrics, SystemFont};
use crate::geometry::{Point, Rect};
use crate::surface::Surface;

/// Colour and pixel size for a run of anti-aliased text.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    pub size_px: f32,
    pub color: Color,
}

impl TextStyle {
    #[must_use]
    pub fn new(size_px: f32, color: Color) -> Self {
        Self { size_px, color }
    }
}

/// The baseline `y` that vertically centres a single line with `metrics` inside
/// `area`. (Ascent is positive, descent negative; see [`LineMetrics`].)
#[must_use]
pub fn centered_baseline(area: Rect, metrics: LineMetrics) -> i32 {
    let text_h = metrics.ascent - metrics.descent;
    (area.origin.y as f32 + (area.size.h as f32 - text_h) * 0.5 + metrics.ascent).round() as i32
}

/// Draw `text` starting at pen `x` on `baseline`, each glyph alpha-blended by its
/// coverage and clipped to `clip`. Returns the pen `x` after the run, so runs
/// chain (e.g. a prompt followed by an editable answer).
pub fn draw_run(
    window: &mut Surface,
    font: &SystemFont,
    clip: Rect,
    text: &str,
    x: i32,
    baseline: i32,
    style: TextStyle,
) -> i32 {
    let mut pen = x;
    for ch in text.chars() {
        let g = font.rasterize(ch, style.size_px);
        let gx = pen + g.xmin;
        let gy = baseline - (g.ymin + g.height as i32);
        for row in 0..g.height {
            for col in 0..g.width {
                let p = Point::new(gx + col as i32, gy + row as i32);
                if clip.contains(p) {
                    window.blend(p, style.color, g.coverage[row * g.width + col]);
                }
            }
        }
        pen += g.advance.round() as i32;
    }
    pen
}

/// Total advance width of `text` at `size_px`, rounding each glyph's advance the
/// same way [`draw_run`] does — so caret and layout maths line up exactly with
/// what is drawn.
#[must_use]
pub fn advance_width(font: &SystemFont, text: &str, size_px: f32) -> i32 {
    text.chars()
        .map(|ch| font.rasterize(ch, size_px).advance.round() as i32)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FontConfig, InputConfig};
    use crate::geometry::Size;

    #[test]
    fn centered_baseline_places_the_line_in_the_middle() {
        let area = Rect::new(Point::new(0, 0), Size::new(100, 40));
        let m = LineMetrics {
            ascent: 16.0,
            descent: -4.0,
            line_height: 20.0,
        };
        // text_h = 20; top gap = (40 - 20) / 2 = 10; baseline = 10 + ascent(16).
        assert_eq!(centered_baseline(area, m), 26);
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn advance_width_is_zero_for_empty_and_grows_with_text() {
        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let px = InputConfig::default().font.size_px;
        assert_eq!(advance_width(&font, "", px), 0);
        let one = advance_width(&font, "x", px);
        let two = advance_width(&font, "xx", px);
        assert!(one > 0 && two > one, "advance grows with text");
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn draw_run_blends_glyphs_and_returns_the_advanced_pen() {
        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let px = InputConfig::default().font.size_px;
        let bg = Color::rgb(0, 0, 0);
        let mut surface = Surface::new(Size::new(120, 40), bg);
        let area = Rect::from_size(surface.size());
        let baseline = centered_baseline(area, font.line_metrics(px));
        let end = draw_run(
            &mut surface,
            &font,
            area,
            "Hi",
            4,
            baseline,
            TextStyle::new(px, Color::rgb(0xFF, 0xFF, 0xFF)),
        );
        assert!(end > 4, "pen advanced past the start");
        assert!(
            surface.as_slice().iter().any(|&w| w != bg.packed()),
            "some pixel was blended above the background"
        );
    }
}
