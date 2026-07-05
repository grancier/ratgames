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

/// The allocation size of a baked banner, measured by
/// [`BigText::footprint`] without allocating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// Cells in the source-resolution grid, before scaling.
    pub source_cells: u64,
    /// Pixels in the final integer-scaled sprite.
    pub scaled_pixels: u64,
}

/// A laid-out banner at source resolution: the glyph masks with their per-glyph
/// blit column (`x_positions[i]`, already shifted so the leftmost content is at
/// 0), and the grid the masks compose into (`cols` × `grid_h`). The single place
/// a banner's geometry is derived, so [`BigText::build_with`] and
/// [`BigText::footprint`] can never disagree.
struct Layout {
    masks: Vec<GlyphMask>,
    x_positions: Vec<i32>,
    cols: u32,
    grid_h: u32,
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

    /// The magnification this builder bakes at — art-pixels per source pixel, the
    /// `scale` from [`new`](Self::new). A baked sprite's glyph cell is this many
    /// times the source [`GlyphSource::cell_height`],
    /// so callers sizing effects off the rendered glyph (e.g. an `em`-relative
    /// drop shadow) must fold it in.
    #[must_use]
    pub fn scale(&self) -> u32 {
        self.scale
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

    /// Lay `text` out into a [`Layout`]: advance the pen by each glyph's own
    /// advance, but size the grid to the true ink bounding box so ink that
    /// overhangs the advance (negative bearings, italic slant) is not clipped.
    fn layout(&self, source: &dyn GlyphSource, text: &str) -> Layout {
        let cell_h = source.cell_height();
        let pad = self.outline_px; // outline head-room reserved on every side
        let masks: Vec<GlyphMask> = text.chars().map(|c| source.glyph(c)).collect();

        // Walk the pen left→right. x = 0 is the first glyph's origin; `ink_left`
        // only drops below it when ink overhangs to the left, and `ink_right`
        // tracks the far edge of the ink or the final pen (whichever is wider).
        let mut pen = 0i32;
        let mut ink_left = 0i32;
        let mut ink_right = 0i32;
        let mut lefts = Vec::with_capacity(masks.len());
        for m in &masks {
            let left = pen + m.x_offset;
            lefts.push(left);
            ink_left = ink_left.min(left);
            ink_right = ink_right.max(left + m.width as i32);
            pen += m.advance as i32 + self.tracking as i32;
        }
        // The trailing advance/tracking is real width before the marquee gap.
        ink_right = ink_right.max(pen);

        // Reserve horizontal head-room so an isolated banner keeps its edge
        // decoration instead of clipping it: the outline extends `outline_px`
        // left of the first glyph, and the outline plus the down-right shadow
        // extend past the last. This mirrors the vertical head-room baked into
        // `grid_h` (a `pad` above, `shadow_depth + pad` below). `gap` is extra
        // trailing space on top, so a scrolling marquee breathes before it repeats.
        let span = (ink_right - ink_left) as u32;
        let left_pad = pad;
        let right_pad = pad + self.shadow_depth;
        let cols = left_pad + span + right_pad + self.gap;
        let grid_h = pad + cell_h + self.shadow_depth + pad;
        // Shift every glyph so the leftmost ink sits at column `left_pad`, leaving
        // the left outline its own column at `left_pad - 1`.
        let x_positions = lefts
            .into_iter()
            .map(|l| l - ink_left + left_pad as i32)
            .collect();
        Layout {
            masks,
            x_positions,
            cols,
            grid_h,
        }
    }

    /// The size a [`build_with`](Self::build_with) bake would allocate, computed
    /// without allocating it: the source-resolution grid (`source_cells`) and the
    /// final scaled sprite (`scaled_pixels`). Lets a caller reject a runaway
    /// banner before the `Vec`s are created; the arithmetic is `u64` so the
    /// measurement itself cannot overflow.
    #[must_use]
    pub fn footprint(&self, source: &dyn GlyphSource, text: &str) -> Footprint {
        let l = self.layout(source, text);
        let source_cells = u64::from(l.cols) * u64::from(l.grid_h);
        let scale = u64::from(self.scale);
        Footprint {
            source_cells,
            scaled_pixels: source_cells * scale * scale,
        }
    }

    /// Rasterise `text` into a sprite using `source` for glyph shapes. The
    /// outline / shadow / integer-scale treatment is identical regardless of
    /// source, so a higher-resolution [`GlyphSource`] yields the same style with
    /// more detail. Unknown chars render blank.
    #[must_use]
    pub fn build_with(&self, source: &dyn GlyphSource, text: &str) -> Sprite {
        let Layout {
            masks,
            x_positions,
            cols,
            grid_h,
        } = self.layout(source, text);
        let pad = self.outline_px; // outline head-room reserved on every side
        let cols_usize = cols as usize;

        // Source-resolution "on" mask; each glyph's ink box blitted at its laid-out
        // column, top-aligned below the outline pad so glyphs share a baseline grid.
        // `layout` guarantees every ink column falls within `[0, cols)`.
        let mut on = vec![false; cols_usize * grid_h as usize];
        for (m, &x0) in masks.iter().zip(&x_positions) {
            for y in 0..m.height {
                for x in 0..m.width {
                    if m.get(x, y) {
                        let sx = (x0 + x as i32) as usize;
                        on[(pad + y) as usize * cols_usize + sx] = true;
                    }
                }
            }
        }

        // Isolated bounds: anything outside the grid reads blank (no wrap). The
        // head-room reserved in `layout` gives the outline and shadow their own
        // columns, and the `Marquee` layer itself tiles a scrolling banner (see
        // `marquee.rs`), so a bake never needs to wrap onto its opposite edge.
        let at = |x: i32, y: i32| -> bool {
            if x < 0 || x >= cols as i32 || y < 0 || y >= grid_h as i32 {
                return false;
            }
            on[y as usize * cols_usize + x as usize]
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
        // span = final pen 18 (2 glyphs advance 8 + tracking 1, last tracking
        // still counts); cols = left_pad(1) + 18 + right_pad(1 + shadow 3) +
        // gap(14) = 37; grid_h = pad(1) + 8 + shadow(3) + pad(1) = 13.
        assert_eq!(sprite.size(), Size::new(37 * scale, 13 * scale));
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
                    x_offset: 0,
                    advance: 3,
                }
            }
        }
        let bt = BigText::new(2).tracking(0).shadow_depth(0).gap(0);
        let sprite = bt.build_with(&Dot, "xx");
        // span = 2 * 3 advance = 6; cols = left_pad(1) + 6 + right_pad(1 + shadow
        // 0) + gap(0) = 8; grid_h = outline(1) + 4 + shadow(0) + outline(1) = 6.
        assert_eq!(sprite.size(), Size::new(8 * 2, 6 * 2));
    }

    #[test]
    fn advance_spaces_glyphs_and_overhang_is_kept() {
        // A source whose ink (4 wide) overhangs its advance (2). Two glyphs are
        // spaced by the advance, so their ink overlaps and tessellates into 6
        // columns — not the 8 that spacing by the ink width would give — and every
        // inked column survives.
        struct Wide;
        impl GlyphSource for Wide {
            fn cell_height(&self) -> u32 {
                4
            }
            fn glyph(&self, _ch: char) -> GlyphMask {
                GlyphMask {
                    width: 4,
                    height: 4,
                    on: vec![true; 16],
                    x_offset: 0,
                    advance: 2,
                }
            }
        }
        let bt = BigText::new(1)
            .tracking(0)
            .shadow_depth(0)
            .gap(0)
            .outline(0);
        let sprite = bt.build_with(&Wide, "xx");
        // pen: 0, 2; ink spans [0,4) and [2,6) -> union [0,6). cols = 6, no gap.
        assert_eq!(sprite.size(), Size::new(6, 4));
        assert_eq!(count(&sprite, palette::FILL), 24, "the whole box is inked");
    }

    #[test]
    fn negative_left_bearing_widens_the_grid() {
        // A glyph whose ink sits 3 px left of the pen origin (a negative bearing).
        // Layout reserves room for that overhang instead of ignoring the offset, so
        // the grid grows leftward by the overhang while the ink itself is preserved.
        struct Bearing(i32);
        impl GlyphSource for Bearing {
            fn cell_height(&self) -> u32 {
                4
            }
            fn glyph(&self, _ch: char) -> GlyphMask {
                GlyphMask {
                    width: 2,
                    height: 4,
                    on: vec![true; 8],
                    x_offset: self.0,
                    advance: 2,
                }
            }
        }
        let bt = BigText::new(1)
            .tracking(0)
            .shadow_depth(0)
            .gap(0)
            .outline(0);
        let flush = bt.build_with(&Bearing(0), "x");
        let overhung = bt.build_with(&Bearing(-3), "x");
        // Flush: ink [0,2), pen_end 2 -> 2 columns. Overhung: ink [-3,-1) with the
        // origin at 0 and pen_end 2 -> span [-3,2) = 5 columns.
        assert_eq!(flush.size(), Size::new(2, 4));
        assert_eq!(overhung.size(), Size::new(5, 4));
        assert_eq!(
            count(&overhung, palette::FILL),
            8,
            "the 2x4 ink is preserved, not dropped"
        );
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

    #[test]
    fn isolated_banner_keeps_left_outline_and_has_no_wrap_bleed() {
        // A single 2x2 all-ink glyph, outlined, with a trailing gap. The first
        // glyph's left outline must survive (it used to be clipped at column 0),
        // and the far-right gap column must stay blank (the old bake wrapped the
        // first glyph's left edge onto the right edge as a spurious outline bar).
        struct Block;
        impl GlyphSource for Block {
            fn cell_height(&self) -> u32 {
                2
            }
            fn glyph(&self, _ch: char) -> GlyphMask {
                GlyphMask {
                    width: 2,
                    height: 2,
                    on: vec![true; 4],
                    x_offset: 0,
                    advance: 2,
                }
            }
        }
        let bt = BigText::new(1)
            .tracking(0)
            .shadow_depth(0)
            .gap(3)
            .outline(1);
        let sprite = bt.build_with(&Block, "x");
        // cols = left_pad(1) + span(2) + right_pad(1 + shadow 0) + gap(3) = 7;
        // grid_h = pad(1) + 2 + shadow(0) + pad(1) = 4.
        assert_eq!(sprite.size(), Size::new(7, 4));
        assert!(
            (0..4).any(|y| sprite.get(Point::new(0, y)) == palette::OUTLINE),
            "the first glyph's left outline must be present, not clipped"
        );
        assert!(
            (0..4).all(|y| sprite.get(Point::new(6, y)) == Color::TRANSPARENT),
            "the trailing gap column must be blank (no wrapped outline bar)"
        );
    }

    #[test]
    fn footprint_matches_a_real_bake() {
        // The measured footprint must equal what `build_with` actually allocates,
        // so the pre-allocation guard reasons about the true size.
        let bt = BigText::new(3)
            .tracking(1)
            .shadow_depth(2)
            .gap(4)
            .outline(1);
        let fp = bt.footprint(&Bitmap8x8, "HI");
        let sz = bt.build_with(&Bitmap8x8, "HI").size();
        assert_eq!(fp.scaled_pixels, u64::from(sz.w) * u64::from(sz.h));
        assert_eq!(fp.source_cells * 3 * 3, fp.scaled_pixels);
    }
}
