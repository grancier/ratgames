//! User-input overlay: an editable line drawn with an anti-aliased monospace
//! font at a fixed device-pixel size.
//!
//! This is intentionally a *separate pipeline* from [`crate::text`]: smooth
//! coverage rather than 1-bit pixels, and device-space rather than the upscaled
//! virtual screen. The editing model is implemented and tested now; glyph
//! rasterisation is stubbed pending a real rasteriser.

use crate::geometry::Rect;
use crate::present::OverlayLayer;
use crate::surface::Surface;

/// On-screen size of the input font, in **device** pixels. The overlay is drawn
/// at this size regardless of the world's integer scale (never upscaled).
pub const INPUT_FONT_PX: f32 = 20.0;

/// An anti-aliased glyph: per-pixel coverage plus horizontal advance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub width: u32,
    pub height: u32,
    /// Row-major coverage, `0` (empty) ..= `255` (solid), length `width*height`.
    pub coverage: Vec<u8>,
    /// Pen advance to the next glyph, in device pixels (rounded).
    pub advance: u32,
}

/// Anti-aliased monospace font for user input.
///
/// **Stub.** Rasterisation is not yet wired. The intended implementation wraps a
/// real rasteriser (e.g. `fontdue`) over embedded TTF bytes and caches glyphs
/// per character — we do not hand-roll glyph anti-aliasing.
#[derive(Debug, Clone)]
pub struct InputFont {
    px: f32,
}

impl InputFont {
    /// Prepare a monospace font to render at `px` device pixels.
    ///
    /// **Stub:** does not yet parse `ttf`.
    #[must_use]
    pub fn from_ttf(_ttf: &[u8], px: f32) -> Self {
        Self { px }
    }

    #[must_use]
    pub fn size_px(&self) -> f32 {
        self.px
    }

    #[must_use]
    pub fn is_monospace(&self) -> bool {
        true
    }

    /// Rasterise `ch` to an anti-aliased coverage bitmap.
    ///
    /// **Stub:** panics until the rasteriser is wired.
    #[must_use]
    pub fn rasterize(&self, _ch: char) -> Glyph {
        unimplemented!("AA glyph rasterisation (fontdue) is not yet wired")
    }
}

/// A single editable line of user input, rendered as an overlay.
///
/// The cursor is a byte index kept on a `char` boundary. Editing is real and
/// tested; [`OverlayLayer::render`] is a no-op stub so the app still runs.
#[derive(Debug, Default, Clone)]
pub struct InputLine {
    buffer: String,
    cursor: usize,
}

impl InputLine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.buffer
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Insert `ch` at the cursor and advance past it.
    pub fn insert(&mut self, ch: char) {
        self.buffer.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    /// Delete the `char` before the cursor. Returns whether anything was removed.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = self.buffer[..self.cursor]
            .chars()
            .next_back()
            .expect("cursor > 0 guarantees a preceding char");
        self.cursor -= prev.len_utf8();
        self.buffer.remove(self.cursor);
        true
    }

    /// Empty the line and reset the cursor.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }
}

impl OverlayLayer for InputLine {
    fn render(&self, _window: &mut Surface, _viewport: Rect) {
        // STUB: blit each char via `InputFont` at `INPUT_FONT_PX`, compositing
        // alpha coverage in device space anchored to `_viewport`. No-op for now
        // so the presentation pipeline runs end-to-end without the rasteriser.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_reports_fixed_unscaled_size() {
        let f = InputFont::from_ttf(&[], INPUT_FONT_PX);
        assert_eq!(f.size_px(), 20.0);
        assert!(f.is_monospace());
    }

    #[test]
    #[should_panic(expected = "not yet wired")]
    fn rasterize_is_stubbed() {
        let _ = InputFont::from_ttf(&[], INPUT_FONT_PX).rasterize('A');
    }

    #[test]
    fn editing_tracks_cursor_across_utf8() {
        let mut line = InputLine::new();
        line.insert('h');
        line.insert('i');
        assert_eq!(line.text(), "hi");
        assert_eq!(line.cursor(), 2);

        // Multi-byte char advances the cursor by its UTF-8 length.
        line.insert('é');
        assert_eq!(line.text(), "hié");
        assert_eq!(line.cursor(), 4);

        assert!(line.backspace());
        assert_eq!(line.text(), "hi");
        assert_eq!(line.cursor(), 2);
    }

    #[test]
    fn backspace_at_start_is_a_noop() {
        let mut line = InputLine::new();
        assert!(!line.backspace());
        assert_eq!(line.text(), "");
    }

    #[test]
    fn clear_resets_buffer_and_cursor() {
        let mut line = InputLine::new();
        line.insert('x');
        line.clear();
        assert_eq!(line.text(), "");
        assert_eq!(line.cursor(), 0);
    }
}
