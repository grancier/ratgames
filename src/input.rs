//! User-input overlay: an editable line drawn with an anti-aliased monospace
//! font inside a nested-border panel.
//!
//! [`InputLine`] is the pure editing model (tested here); [`InputField`] is the
//! [`OverlayLayer`] that lays it out and rasterises it in **device** space —
//! the smooth-text pipeline, never pixel-scaled. Everything visual comes from
//! [`InputConfig`]; there are no literals in the rendering.

use crate::config::InputConfig;
use crate::font::SystemFont;
use crate::geometry::{Point, Rect, Size};
use crate::present::OverlayLayer;
use crate::surface::Surface;

/// A single editable line of text. The cursor is a byte index kept on a `char`
/// boundary.
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

/// The bottom input panel: nested border + anti-aliased text, all from config.
#[derive(Debug)]
pub struct InputField {
    line: InputLine,
    config: InputConfig,
    font: SystemFont,
}

impl InputField {
    #[must_use]
    pub fn new(config: InputConfig, font: SystemFont) -> Self {
        Self {
            line: InputLine::new(),
            config,
            font,
        }
    }

    #[must_use]
    pub fn line(&self) -> &InputLine {
        &self.line
    }

    /// Insert a typed character. Control codes are ignored here — backspace and
    /// submit are explicit ([`backspace`](Self::backspace) / [`submit`](Self::submit)).
    pub fn type_char(&mut self, ch: char) {
        if !ch.is_control() {
            self.line.insert(ch);
        }
    }

    pub fn backspace(&mut self) {
        self.line.backspace();
    }

    /// Commit the line. For now it just clears; a real handler would forward the
    /// text to a command layer.
    pub fn submit(&mut self) {
        self.line.clear();
    }

    /// Rasterise the current text into `area`, clipped to it, with a caret.
    fn draw_text(&self, window: &mut Surface, area: Rect) {
        let px = self.config.font.size_px;
        let lm = self.font.line_metrics(px);
        let text_h = lm.ascent - lm.descent;
        let baseline =
            (area.origin.y as f32 + (area.size.h as f32 - text_h) * 0.5 + lm.ascent).round() as i32;

        let mut pen = area.origin.x;
        let mut caret_x = pen;
        let mut byte = 0usize;
        for ch in self.line.text().chars() {
            if byte == self.line.cursor() {
                caret_x = pen;
            }
            let g = self.font.rasterize(ch, px);
            let gx = pen + g.xmin;
            let gy = baseline - (g.ymin + g.height as i32);
            for row in 0..g.height {
                for col in 0..g.width {
                    let p = Point::new(gx + col as i32, gy + row as i32);
                    if area.contains(p) {
                        window.blend(p, self.config.text_color, g.coverage[row * g.width + col]);
                    }
                }
            }
            pen += g.advance.round() as i32;
            byte += ch.len_utf8();
        }
        if byte == self.line.cursor() {
            caret_x = pen;
        }

        // Caret, clamped inside the text area.
        let cw = self.config.caret_width_px as i32;
        let hi = (area.right() - cw).max(area.origin.x);
        let caret = Rect::new(
            Point::new(caret_x.clamp(area.origin.x, hi), area.origin.y),
            Size::new(self.config.caret_width_px, area.size.h),
        );
        window.fill_rect(caret, self.config.text_color);
    }
}

impl OverlayLayer for InputField {
    fn render(&self, window: &mut Surface, _viewport: Rect) {
        let layout = self.config.layout(window.size());
        window.fill_rect(layout.panel, self.config.background_color);
        for border in &layout.borders {
            window.draw_rect_outline(
                *border,
                self.config.border.color,
                self.config.border.line_thickness_px,
            );
        }
        self.draw_text(window, layout.text_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_tracks_cursor_across_utf8() {
        let mut line = InputLine::new();
        line.insert('h');
        line.insert('i');
        assert_eq!(line.text(), "hi");
        assert_eq!(line.cursor(), 2);

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
