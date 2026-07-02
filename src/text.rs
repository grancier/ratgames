//! Pixel-art text: oversized "8-bit" glyphs baked into a reusable [`Sprite`].
//!
//! This is the *pixel* text path (1-bit alpha, integer-scaled with the world).
//! The anti-aliased input font is a different pipeline entirely — see
//! [`crate::input`].

use crate::color::{Color, palette};
use crate::geometry::{Point, Size};
use crate::glyph::{Bitmap8x8, GlyphMask, GlyphSource};
use crate::sprite::Sprite;

/// What to lay down at a single art-pixel of a glyph. A closed state machine
/// resolved by fixed precedence: `Fill` > `Outline` > `Shadow` > `Transparent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Transparent,
    Shadow,
    Outline,
    Fill,
}

/// The three ink colours of big text. Defaults to the retro palette; override
/// per banner via [`BigText::colors`]. This is where "green / black / yellow"
/// lives as configurable data rather than baked into the rasteriser.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TextColors {
    pub fill: Color,
    pub outline: Color,
    pub shadow: Color,
}

impl Default for TextColors {
    fn default() -> Self {
        Self {
            fill: palette::FILL,
            outline: palette::OUTLINE,
            shadow: palette::SHADOW,
        }
    }
}

impl TextColors {
    /// Resolve an [`Ink`] to its colour.
    #[must_use]
    pub fn of(self, ink: Ink) -> Color {
        match ink {
            Ink::Transparent => Color::TRANSPARENT,
            Ink::Shadow => self.shadow,
            Ink::Outline => self.outline,
            Ink::Fill => self.fill,
        }
    }
}

/// Builder for oversized bitmap text: green fill, black outline, extruded
/// yellow shadow. Produces a [`Sprite`]; scrolling/placement is the caller's
/// concern (see [`crate::marquee::Marquee`]), which keeps this a pure factory.
#[derive(Debug, Clone, Copy)]
pub struct BigText {
    scale: u32,
    tracking: u32,
    shadow_depth: u32,
    outline_px: u32,
    gap: u32,
    colors: TextColors,
}

impl Default for BigText {
    fn default() -> Self {
        Self {
            scale: 6,
            tracking: 1,
            shadow_depth: 3,
            outline_px: 1,
            gap: 14,
            colors: TextColors::default(),
        }
    }
}

impl BigText {
    /// A builder at `scale` art-pixels per font-pixel, other knobs defaulted.
    #[must_use]
    pub fn new(scale: u32) -> Self {
        Self {
            scale: scale.max(1),
            ..Self::default()
        }
    }

    /// Blank font-columns between glyphs.
    #[must_use]
    pub fn tracking(mut self, tracking: u32) -> Self {
        self.tracking = tracking;
        self
    }

    /// Depth of the down-right shadow extrusion, in font-pixels.
    #[must_use]
    pub fn shadow_depth(mut self, depth: u32) -> Self {
        self.shadow_depth = depth;
        self
    }

    /// Outline thickness around the fill, in source pixels (`0` = no outline).
    #[must_use]
    pub fn outline(mut self, outline_px: u32) -> Self {
        self.outline_px = outline_px;
        self
    }

    /// Trailing blank font-columns, so a marquee has a gap before it repeats.
    #[must_use]
    pub fn gap(mut self, gap: u32) -> Self {
        self.gap = gap;
        self
    }

    /// Override the fill / outline / shadow colours.
    #[must_use]
    pub fn colors(mut self, colors: TextColors) -> Self {
        self.colors = colors;
        self
    }

    /// Rasterise `text` into a sprite with the default 8×8 source
    /// ([`Bitmap8x8`]). Unknown / non-ASCII chars render blank.
    #[must_use]
    pub fn build(&self, text: &str) -> Sprite {
        self.build_with(&Bitmap8x8, text)
    }

    /// Rasterise `text` into a sprite using `source` for glyph shapes. The
    /// outline / shadow / integer-scale treatment is identical regardless of
    /// source, so a higher-resolution [`GlyphSource`] yields the same style with
    /// more detail. Unknown chars render blank.
    #[must_use]
    pub fn build_with(&self, source: &dyn GlyphSource, text: &str) -> Sprite {
        let cell_h = source.cell_height();
        let pad = self.outline_px; // outline head-room reserved on every side
        let masks: Vec<GlyphMask> = text.chars().map(|c| source.glyph(c)).collect();
        let cols: u32 = masks.iter().map(|m| m.width + self.tracking).sum::<u32>() + self.gap;
        let grid_h = pad + cell_h + self.shadow_depth + pad;
        let cols_usize = cols as usize;

        // Source-resolution "on" mask; glyphs laid left to right, top-aligned so
        // they share a baseline grid.
        let mut on = vec![false; cols_usize * grid_h as usize];
        let mut x0 = 0u32;
        for m in &masks {
            for y in 0..m.height {
                for x in 0..m.width {
                    if m.get(x, y) {
                        on[(pad + y) as usize * cols_usize + (x0 + x) as usize] = true;
                    }
                }
            }
            x0 += m.width + self.tracking;
        }

        // Horizontal wrap makes the baked sprite tile seamlessly for a marquee.
        let at = |x: i32, y: i32| -> bool {
            if y < 0 || y >= grid_h as i32 {
                return false;
            }
            let xm = x.rem_euclid(cols as i32) as usize;
            on[y as usize * cols_usize + xm]
        };
        let depth = self.shadow_depth as i32;
        let ink_at = |x: i32, y: i32| -> Ink {
            if at(x, y) {
                Ink::Fill
            } else if outline_hit(&at, x, y, self.outline_px as i32) {
                Ink::Outline
            } else if is_shadow(&at, x, y, depth) {
                Ink::Shadow
            } else {
                Ink::Transparent
            }
        };

        // Bake into a scaled sprite (source-pixel -> scale × scale block).
        let scale = self.scale;
        let mut sprite = Sprite::new(Size::new(cols * scale, grid_h * scale));
        for gy in 0..grid_h as i32 {
            for gx in 0..cols as i32 {
                let color = self.colors.of(ink_at(gx, gy));
                if !color.is_visible() {
                    continue;
                }
                for dy in 0..scale as i32 {
                    for dx in 0..scale as i32 {
                        sprite.set(
                            Point::new(gx * scale as i32 + dx, gy * scale as i32 + dy),
                            color,
                        );
                    }
                }
            }
        }
        sprite
    }
}

/// Whether any fill pixel lies within Chebyshev distance `radius` of (`x`, `y`) —
/// i.e. whether an `radius`-thick outline should ink this cell. `radius = 0`
/// never hits, so `outline_px = 0` means no outline.
fn outline_hit(at: &impl Fn(i32, i32) -> bool, x: i32, y: i32, radius: i32) -> bool {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if (dx, dy) != (0, 0) && at(x + dx, y + dy) {
                return true;
            }
        }
    }
    false
}

fn is_shadow(at: &impl Fn(i32, i32) -> bool, x: i32, y: i32, depth: i32) -> bool {
    (1..=depth).any(|k| at(x - k, y - k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(sprite: &Sprite, color: Color) -> usize {
        let s = sprite.size();
        let mut n = 0;
        for y in 0..s.h as i32 {
            for x in 0..s.w as i32 {
                if sprite.get(Point::new(x, y)) == color {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn default_text_colors_match_palette() {
        let c = TextColors::default();
        assert_eq!(c.of(Ink::Fill), palette::FILL);
        assert_eq!(c.of(Ink::Outline), palette::OUTLINE);
        assert_eq!(c.of(Ink::Shadow), palette::SHADOW);
        assert_eq!(c.of(Ink::Transparent), Color::TRANSPARENT);
    }

    #[test]
    fn custom_colors_are_honoured() {
        let pink = Color::rgb(255, 0, 255);
        let sprite = BigText::new(3)
            .colors(TextColors {
                fill: pink,
                ..TextColors::default()
            })
            .build("A");
        assert!(count(&sprite, pink) > 0);
        assert_eq!(count(&sprite, palette::FILL), 0);
    }

    #[test]
    fn build_produces_all_three_layers_over_transparency() {
        let sprite = BigText::new(4).build("A");
        assert!(count(&sprite, palette::FILL) > 0, "expected green fill");
        assert!(
            count(&sprite, palette::OUTLINE) > 0,
            "expected black outline"
        );
        assert!(
            count(&sprite, palette::SHADOW) > 0,
            "expected yellow shadow"
        );
        assert!(
            count(&sprite, Color::TRANSPARENT) > 0,
            "expected transparent bg"
        );
    }

    #[test]
    fn sprite_dimensions_follow_scale() {
        let scale = 5;
        let text = BigText::new(scale).tracking(1).shadow_depth(3).gap(14);
        let sprite = text.build("HI");
        // cols = 2 glyphs * (8 + 1) + 14 = 32; grid_h = 1 + 8 + 3 + 1 = 13.
        assert_eq!(sprite.size(), Size::new(32 * scale, 13 * scale));
    }

    #[test]
    fn unknown_glyph_is_blank() {
        let sprite = BigText::new(3).build("é");
        assert_eq!(count(&sprite, palette::FILL), 0);
        assert_eq!(count(&sprite, palette::OUTLINE), 0);
    }

    #[test]
    fn build_with_lays_out_by_the_source_metrics() {
        // A trivial source proves BigText is glyph-source-agnostic: a 3-wide,
        // 4-tall all-ink glyph lays out by the source's dimensions, not 8x8.
        struct Dot;
        impl GlyphSource for Dot {
            fn cell_height(&self) -> u32 {
                4
            }
            fn glyph(&self, _ch: char) -> GlyphMask {
                GlyphMask {
                    width: 3,
                    height: 4,
                    on: vec![true; 12],
                }
            }
        }
        let bt = BigText::new(2).tracking(0).shadow_depth(0).gap(0);
        let sprite = bt.build_with(&Dot, "xx");
        // cols = 2 * (3 + 0) + 0 = 6; grid_h = outline(1) + 4 + 0 + outline(1) = 6.
        assert_eq!(sprite.size(), Size::new(6 * 2, 6 * 2));
    }

    #[test]
    fn outline_px_thickens_the_border() {
        let thin = BigText::new(1).shadow_depth(0).gap(0).outline(1).build("A");
        let thick = BigText::new(1).shadow_depth(0).gap(0).outline(3).build("A");
        assert!(
            count(&thick, palette::OUTLINE) > count(&thin, palette::OUTLINE),
            "a thicker outline should ink more border cells"
        );
    }

    #[test]
    fn outline_zero_draws_no_border() {
        let none = BigText::new(2).shadow_depth(0).gap(0).outline(0).build("A");
        assert_eq!(count(&none, palette::OUTLINE), 0);
        assert!(count(&none, palette::FILL) > 0, "fill still present");
    }
}
