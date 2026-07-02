//! Glyph sources for the pixel-art text renderer.
//!
//! [`BigText`](crate::text::BigText) is source-agnostic: it lays out, outlines,
//! shadows, and integer-scales a grid of 1-bit "on" cells. A [`GlyphSource`]
//! supplies those cells per character, so the *look* (outline + shadow + crisp
//! upscale) is fixed while the *detail* comes from the source. `font8x8` is one
//! source ([`Bitmap8x8`], the chunky retro default); a TTF rasterised and
//! thresholded is another (`RasterGlyphSource`), giving the same style at higher
//! resolution.

use crate::font::SystemFont;
use font8x8::legacy::BASIC_LEGACY;

/// One character's 1-bit coverage inside a fixed-height cell.
///
/// `on` is row-major, length `width * height`, `true` = ink. `height` equals the
/// source's [`cell_height`](GlyphSource::cell_height) so every glyph shares a
/// baseline grid; `width` may vary per glyph (proportional fonts).
#[derive(Debug, Clone)]
pub struct GlyphMask {
    pub width: u32,
    pub height: u32,
    pub on: Vec<bool>,
}

impl GlyphMask {
    /// A blank `width × height` cell (all off) — the fallback for an unknown
    /// character.
    #[must_use]
    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            on: vec![false; (width * height) as usize],
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

        // The mask is the glyph's advance wide (proportional spacing); a blank
        // char like space still advances, yielding an empty cell of that width.
        let width = (g.advance.round() as i32).max(1) as u32;
        let mut on = vec![false; (width * cell_h) as usize];

        // Place the coverage bitmap on the baseline (y-up: the top row is
        // baseline - (ymin + height)), thresholded and clipped to the cell.
        let top = baseline - (g.ymin + g.height as i32);
        for row in 0..g.height {
            for col in 0..g.width {
                if g.coverage[row * g.width + col] >= self.threshold {
                    let cx = g.xmin + col as i32;
                    let cy = top + row as i32;
                    if cx >= 0 && (cx as u32) < width && cy >= 0 && (cy as u32) < cell_h {
                        on[cy as usize * width as usize + cx as usize] = true;
                    }
                }
            }
        }

        GlyphMask {
            width,
            height: cell_h,
            on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::FontConfig, font::SystemFont, text::BigText};

    #[test]
    fn bitmap8x8_cell_is_eight_tall() {
        assert_eq!(Bitmap8x8.cell_height(), 8);
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

        let space = src.glyph(' ');
        assert!(space.width > 0, "space advances the pen");
        assert!(space.on.iter().all(|&b| !b), "space is blank");
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
