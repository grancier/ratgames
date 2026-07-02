//! Glyph sources for the pixel-art text renderer.
//!
//! [`BigText`](crate::text::BigText) is source-agnostic: it lays out, outlines,
//! shadows, and integer-scales a grid of 1-bit "on" cells. A [`GlyphSource`]
//! supplies those cells per character, so the *look* (outline + shadow + crisp
//! upscale) is fixed while the *detail* comes from the source. `font8x8` is one
//! source ([`Bitmap8x8`], the chunky retro default); a TTF rasterised and
//! thresholded is another (`RasterGlyphSource`), giving the same style at higher
//! resolution.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
