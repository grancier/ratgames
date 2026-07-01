//! Pixel-art text: oversized "8-bit" glyphs baked into a reusable [`Sprite`].
//!
//! This is the *pixel* text path (1-bit alpha, integer-scaled with the world).
//! The anti-aliased input font is a different pipeline entirely — see
//! [`crate::input`].

use crate::color::{palette, Color};
use crate::geometry::{Point, Size};
use crate::sprite::Sprite;
use font8x8::legacy::BASIC_LEGACY;

const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 8;
/// Outline head-room reserved above the glyph rows inside the working grid.
const PAD_TOP: u32 = 1;

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
#[derive(Debug, Clone, Copy)]
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
    gap: u32,
    colors: TextColors,
}

impl Default for BigText {
    fn default() -> Self {
        Self {
            scale: 6,
            tracking: 1,
            shadow_depth: 3,
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

    /// Rasterise `text` into a sprite. Unknown / non-ASCII chars render blank.
    #[must_use]
    pub fn build(&self, text: &str) -> Sprite {
        let glyphs: Vec<[u8; 8]> = text.chars().map(glyph_bits).collect();
        let cols = glyphs.len() as u32 * (GLYPH_W + self.tracking) + self.gap;
        let grid_h = PAD_TOP + GLYPH_H + self.shadow_depth + 1;
        let cols_usize = cols as usize;

        // Font-resolution "on" mask.
        let mut on = vec![false; cols_usize * grid_h as usize];
        for (i, g) in glyphs.iter().enumerate() {
            let x0 = i as u32 * (GLYPH_W + self.tracking);
            for (row, bits) in g.iter().enumerate() {
                for c in 0..GLYPH_W {
                    // font8x8: bit 0 (LSB) is the leftmost column.
                    if (bits >> c) & 1 == 1 {
                        let x = (x0 + c) as usize;
                        let y = (PAD_TOP + row as u32) as usize;
                        on[y * cols_usize + x] = true;
                    }
                }
            }
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
            } else if neighbour_is_fill(&at, x, y) {
                Ink::Outline
            } else if is_shadow(&at, x, y, depth) {
                Ink::Shadow
            } else {
                Ink::Transparent
            }
        };

        // Bake into a scaled sprite (font-pixel -> scale × scale block).
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
                        sprite.set(Point::new(gx * scale as i32 + dx, gy * scale as i32 + dy), color);
                    }
                }
            }
        }
        sprite
    }
}

/// The 8×8 bitmap for `c`, or a blank cell for anything outside ASCII.
fn glyph_bits(c: char) -> [u8; 8] {
    let i = c as usize;
    if i < BASIC_LEGACY.len() {
        BASIC_LEGACY[i]
    } else {
        [0; 8]
    }
}

fn neighbour_is_fill(at: &impl Fn(i32, i32) -> bool, x: i32, y: i32) -> bool {
    for dy in -1..=1 {
        for dx in -1..=1 {
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
        assert!(count(&sprite, palette::OUTLINE) > 0, "expected black outline");
        assert!(count(&sprite, palette::SHADOW) > 0, "expected yellow shadow");
        assert!(count(&sprite, Color::TRANSPARENT) > 0, "expected transparent bg");
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
}
