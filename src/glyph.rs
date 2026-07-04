//! Glyph sources for the pixel-art text renderer.
//!
//! [`BigText`](crate::text::BigText) is source-agnostic: it lays out, outlines,
//! shadows, and integer-scales a grid of 1-bit "on" cells. A [`GlyphSource`]
//! supplies those cells per character, so the *look* (outline + shadow + crisp
//! upscale) is fixed while the *detail* comes from the source. `font8x8` is one
//! source ([`Bitmap8x8`], the chunky retro default); a TTF rasterised and
//! thresholded is another (`RasterGlyphSource`), giving the same style at higher
//! resolution.

use crate::color::Color;
use crate::font::SystemFont;
use crate::geometry::{Point, Size};
use crate::sprite::Sprite;
use font8x8::legacy::BASIC_LEGACY;

/// One character's 1-bit coverage box plus its placement on the pen.
///
/// `on` is row-major, length `width * height`, `true` = ink. `height` equals the
/// source's [`cell_height`](GlyphSource::cell_height) so every glyph shares a
/// baseline grid; `width` is the glyph's own ink width (proportional). The box is
/// positioned by [`x_offset`](Self::x_offset) — its left edge relative to the pen
/// origin, i.e. the left side bearing, which may be negative — while the pen moves
/// on by [`advance`](Self::advance). Separating the ink box from the advance lets a
/// glyph's ink overhang the advance (italic slant, negative bearings) without being
/// clipped to it.
#[derive(Debug, Clone)]
pub struct GlyphMask {
    pub width: u32,
    pub height: u32,
    pub on: Vec<bool>,
    /// Horizontal offset from the pen origin to the ink box's left edge (the left
    /// side bearing). Negative when ink precedes the origin.
    pub x_offset: i32,
    /// Horizontal pen advance to the next glyph's origin.
    pub advance: u32,
}

impl GlyphMask {
    /// A blank `width × height` cell (all off) that advances by its own width —
    /// the fallback for an unknown character.
    #[must_use]
    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            on: vec![false; (width * height) as usize],
            x_offset: 0,
            advance: width,
        }
    }

    /// Whether the pixel at (`x`, `y`) is ink. Out-of-bounds reads as off.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.on[(y * self.width + x) as usize]
    }

    /// Bake this mask into a [`Sprite`] in `ink`, cropped to its ink bounds so a
    /// lone glyph carries no surrounding padding and centres on its own marks. A
    /// mask with no ink yields a 1×1 transparent sprite.
    ///
    /// This bypasses [`BigText`](crate::text::BigText) layout — no side bearing, no
    /// outline, no drop shadow — which is exactly what a single symbol (an icon, a
    /// reject cross) wants: `BigText`'s padding would push a lone glyph off-centre
    /// and its shadow would merge into a blob.
    #[must_use]
    pub fn to_sprite(&self, ink: Color) -> Sprite {
        let (mut x0, mut y0, mut x1, mut y1) = (self.width, self.height, 0, 0);
        let mut any = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) {
                    any = true;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if !any {
            return Sprite::new(Size::new(1, 1));
        }
        let mut sprite = Sprite::new(Size::new(x1 - x0 + 1, y1 - y0 + 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                if self.get(x, y) {
                    sprite.set(Point::new((x - x0) as i32, (y - y0) as i32), ink);
                }
            }
        }
        sprite
    }
}

/// A provider of pixel-art glyph masks for [`BigText`](crate::text::BigText).
pub trait GlyphSource {
    /// The common cell height, in source pixels; every [`glyph`](Self::glyph)
    /// mask is this tall.
    fn cell_height(&self) -> u32;

    /// The 1-bit mask for `ch`. Unknown characters return a blank cell.
    fn glyph(&self, ch: char) -> GlyphMask;
}

/// The classic 8×8 source: `font8x8`'s ASCII table, one bit per pixel — the
/// chunky retro default.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bitmap8x8;

impl GlyphSource for Bitmap8x8 {
    fn cell_height(&self) -> u32 {
        8
    }

    fn glyph(&self, ch: char) -> GlyphMask {
        let i = ch as usize;
        let bits = if i < BASIC_LEGACY.len() {
            BASIC_LEGACY[i]
        } else {
            [0u8; 8]
        };
        let mut on = vec![false; 8 * 8];
        for (row, byte) in bits.iter().enumerate() {
            for col in 0..8u32 {
                // font8x8: bit 0 (LSB) is the leftmost column.
                if (byte >> col) & 1 == 1 {
                    on[row * 8 + col as usize] = true;
                }
            }
        }
        GlyphMask {
            width: 8,
            height: 8,
            on,
            x_offset: 0,
            advance: 8,
        }
    }
}

/// A higher-resolution source: a real font rasterised at `cell_px` and
/// thresholded to 1-bit, so [`BigText`](crate::text::BigText)'s outline / shadow
/// / integer-scale treatment applies to crisp, detailed glyphs instead of an 8×8
/// grid. Proportional — each glyph's mask is its advance wide.
#[derive(Debug)]
pub struct RasterGlyphSource {
    font: SystemFont,
    cell_px: u32,
    threshold: u8,
}

impl RasterGlyphSource {
    /// Rasterise `font` at `cell_px` source-pixels per em (clamped to ≥ 1);
    /// coverage ≥ 128 becomes ink.
    #[must_use]
    pub fn new(font: SystemFont, cell_px: u32) -> Self {
        Self {
            font,
            cell_px: cell_px.max(1),
            threshold: 128,
        }
    }

    /// Set the coverage cut-off (`0..=255`) above which a pixel is ink. Lower
    /// keeps more of the anti-aliased edge (heavier glyphs); higher is thinner.
    #[must_use]
    pub fn with_threshold(mut self, threshold: u8) -> Self {
        self.threshold = threshold;
        self
    }
}

impl GlyphSource for RasterGlyphSource {
    fn cell_height(&self) -> u32 {
        let m = self.font.line_metrics(self.cell_px as f32);
        ((m.ascent - m.descent).ceil() as u32).max(1)
    }

    fn glyph(&self, ch: char) -> GlyphMask {
        let px = self.cell_px as f32;
        let cell_h = self.cell_height();
        let baseline = self.font.line_metrics(px).ascent.round() as i32;
        let g = self.font.rasterize(ch, px);

        // The ink box is exactly the rasterised bitmap's width — no longer the
        // rounded advance — so ink is never clipped to the advance. The left side
        // bearing travels as `x_offset` and the pen advance as `advance`, so the
        // layout in `BigText` can place the ink and still space proportionally. A
        // blank char (space) has no bitmap, so its box is empty and only the
        // advance carries the spacing.
        let width = g.width as u32;
        let advance = g.advance.round().max(0.0) as u32;
        let mut on = vec![false; (width * cell_h) as usize];

        // Drop the coverage bitmap onto the baseline (y-up: the top row is
        // baseline - (ymin + height)), thresholded. Horizontal ink is preserved in
        // full; only the vertical extent is clipped to the cell.
        let top = baseline - (g.ymin + g.height as i32);
        for row in 0..g.height {
            for col in 0..g.width {
                if g.coverage[row * g.width + col] >= self.threshold {
                    let cy = top + row as i32;
                    if cy >= 0 && (cy as u32) < cell_h {
                        on[cy as usize * width as usize + col] = true;
                    }
                }
            }
        }

        GlyphMask {
            width,
            height: cell_h,
            on,
            x_offset: g.xmin,
            advance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{FontConfig, FontFamily, FontSource, FontStretch, FontStyle, FontWeight},
        font::SystemFont,
        text::BigText,
    };

    #[test]
    fn bitmap8x8_cell_is_eight_tall() {
        assert_eq!(Bitmap8x8.cell_height(), 8);
    }

    #[test]
    fn bitmap_glyph_carries_advance_and_zero_bearing() {
        // The 8×8 source is fixed-pitch: the ink box, advance, and cell all agree,
        // and there is no side bearing — the baseline case for the box/advance split.
        let a = Bitmap8x8.glyph('A');
        assert_eq!(a.width, 8);
        assert_eq!(a.advance, 8, "a full cell of advance");
        assert_eq!(a.x_offset, 0, "no side bearing");
    }

    #[test]
    fn known_glyph_has_ink_and_unknown_is_blank() {
        let a = Bitmap8x8.glyph('A');
        assert_eq!((a.width, a.height), (8, 8));
        assert!(a.on.iter().any(|&b| b), "'A' has ink");

        let unknown = Bitmap8x8.glyph('é'); // outside BASIC_LEGACY's 128 entries
        assert!(unknown.on.iter().all(|&b| !b), "non-ASCII renders blank");
    }

    #[test]
    fn blank_mask_is_all_off_and_get_clamps() {
        let m = GlyphMask::blank(3, 4);
        assert_eq!((m.width, m.height), (3, 4));
        assert!(m.on.iter().all(|&b| !b));
        assert!(!m.get(0, 0));
        assert!(!m.get(99, 99)); // out of bounds -> off, no panic
    }

    #[test]
    fn to_sprite_crops_a_glyph_to_its_ink() {
        // Straight from the 8×8 mask, cropped to ink: a proper X, not a padded,
        // off-centre blob (the reason a single symbol skips BigText layout).
        let red = Color::rgb(0xE0, 0x2C, 0x2C);
        let sprite = Bitmap8x8.glyph('X').to_sprite(red);
        assert_eq!(sprite.size(), Size::new(7, 7)); // trimmed to the X's ink bounds
        assert_eq!(sprite.get(Point::new(0, 0)), red); // top-left arm
        assert_eq!(sprite.get(Point::new(6, 6)), red); // bottom-right arm
        assert_eq!(sprite.get(Point::new(3, 3)), red); // the crossing
        assert!(!sprite.get(Point::new(3, 0)).is_visible()); // gap between the top arms
    }

    #[test]
    fn to_sprite_of_a_blank_mask_is_one_transparent_pixel() {
        let sprite = GlyphMask::blank(8, 8).to_sprite(Color::rgb(1, 2, 3));
        assert_eq!(sprite.size(), Size::new(1, 1));
        assert!(!sprite.get(Point::new(0, 0)).is_visible());
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn raster_source_has_a_tall_cell_and_inks_glyphs() {
        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let src = RasterGlyphSource::new(font, 24);
        assert!(
            src.cell_height() >= 24,
            "cell height tracks the raster size"
        );

        let a = src.glyph('A');
        assert_eq!(a.height, src.cell_height());
        assert!(a.width > 0 && a.on.iter().any(|&b| b), "'A' has ink");
        assert!(a.advance > 0, "'A' advances the pen");

        // A blank char has an empty ink box; only the advance carries the spacing.
        let space = src.glyph(' ');
        assert!(space.advance > 0, "space advances the pen");
        assert!(space.on.iter().all(|&b| !b), "space is blank");
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn raster_glyph_box_is_the_ink_not_the_advance() {
        // The ink box tracks the rasterised glyph while the side bearing and pen
        // advance travel as placement metadata, so no coverage is clipped to the
        // advance. Every thresholded pixel inside the cell's vertical bounds
        // survives — the horizontal clip the old advance-width mask applied is gone.
        let probe = SystemFont::load(&FontConfig::default()).expect("a system font");
        let g = probe.rasterize('W', 32.0);
        let src = RasterGlyphSource::new(
            SystemFont::load(&FontConfig::default()).expect("a system font"),
            32,
        );
        let m = src.glyph('W');

        assert_eq!(m.width, g.width as u32, "the box is the ink's own width");
        assert_eq!(m.x_offset, g.xmin, "the side bearing travels as x_offset");
        assert_eq!(
            m.advance,
            g.advance.round() as u32,
            "the pen advance is separate from the box"
        );

        let cell_h = src.cell_height();
        let baseline = probe.line_metrics(32.0).ascent.round() as i32;
        let top = baseline - (g.ymin + g.height as i32);
        let expected = (0..g.height)
            .flat_map(|row| (0..g.width).map(move |col| (row, col)))
            .filter(|&(row, col)| g.coverage[row * g.width + col] >= 128)
            .filter(|&(row, _)| {
                let cy = top + row as i32;
                cy >= 0 && (cy as u32) < cell_h
            })
            .count();
        assert_eq!(
            m.on.iter().filter(|&&b| b).count(),
            expected,
            "no horizontal ink is clipped away"
        );
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn italic_faces_produce_overhanging_ink() {
        // Motivation for the box/advance split: a slanted face rasterises ink that
        // overhangs the advance or precedes the origin. Such a glyph must exist for
        // the preservation to matter — PR #7 made these faces reachable.
        let font = SystemFont::from_source(&FontSource::System {
            family: FontFamily::Named("Menlo".to_string()),
            weight: FontWeight(400),
            style: FontStyle::Italic,
            stretch: FontStretch::Normal,
        })
        .expect("Menlo Italic");
        let src = RasterGlyphSource::new(font, 32);
        let overhangs = ('!'..='~').any(|ch| {
            let m = src.glyph(ch);
            m.x_offset < 0 || m.x_offset + m.width as i32 > m.advance as i32
        });
        assert!(
            overhangs,
            "an italic face should overhang its advance somewhere"
        );
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn raster_beats_8x8_resolution_through_big_text() {
        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let raster = RasterGlyphSource::new(font, 24);
        let bt = BigText::new(1).shadow_depth(0).gap(0);
        // Same scale (1): the raster cell (~24+ tall) beats the 8x8 source's
        // height — more actual detail, not just magnification.
        assert!(
            bt.build_with(&raster, "A").size().h > bt.build_with(&Bitmap8x8, "A").size().h,
            "raster source is higher resolution than font8x8"
        );
    }
}
