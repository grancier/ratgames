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
use crate::overlay;
use crate::present::OverlayLayer;
use crate::surface::Surface;
use crate::ui::UiInput;

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

    /// Move the caret one char left. Returns whether it moved.
    pub fn left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = self.buffer[..self.cursor]
            .chars()
            .next_back()
            .expect("cursor > 0 guarantees a preceding char");
        self.cursor -= prev.len_utf8();
        true
    }

    /// Move the caret one char right. Returns whether it moved.
    pub fn right(&mut self) -> bool {
        if self.cursor >= self.buffer.len() {
            return false;
        }
        let next = self.buffer[self.cursor..]
            .chars()
            .next()
            .expect("cursor < len guarantees a following char");
        self.cursor += next.len_utf8();
        true
    }

    /// Delete the char at the caret (forward delete); the caret stays put.
    /// Returns whether a char was removed.
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.buffer.len() {
            return false;
        }
        self.buffer.remove(self.cursor);
        true
    }

    /// Move the caret to the start. Returns whether it moved.
    pub fn home(&mut self) -> bool {
        let moved = self.cursor != 0;
        self.cursor = 0;
        moved
    }

    /// Move the caret to the end. Returns whether it moved.
    pub fn end(&mut self) -> bool {
        let moved = self.cursor != self.buffer.len();
        self.cursor = self.buffer.len();
        moved
    }

    /// Apply an editing [`UiInput`] — `Char`/`Backspace`/`Delete`, the arrows,
    /// and `Home`/`End`. Selection and commit inputs (`Up`/`Down`/`Confirm`/
    /// `Cancel`) are ignored so a caller can route the same event stream to both
    /// a field and a menu. Returns whether the text or caret changed.
    pub fn handle(&mut self, input: UiInput) -> bool {
        match input {
            UiInput::Char(c) if !c.is_control() => {
                self.insert(c);
                true
            }
            UiInput::Backspace => self.backspace(),
            UiInput::Delete => self.delete(),
            UiInput::Left => self.left(),
            UiInput::Right => self.right(),
            UiInput::Home => self.home(),
            UiInput::End => self.end(),
            _ => false,
        }
    }
}

/// The bottom input panel: nested border + anti-aliased text, all from config.
#[derive(Debug)]
pub struct InputField {
    line: InputLine,
    prompt: String,
    config: InputConfig,
    font: SystemFont,
}

impl InputField {
    #[must_use]
    pub fn new(config: InputConfig, font: SystemFont) -> Self {
        Self {
            line: InputLine::new(),
            prompt: String::new(),
            config,
            font,
        }
    }

    /// Set a fixed prompt drawn before the editable answer, in the same font and
    /// the configured `prompt_color`. Builder form.
    #[must_use]
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Replace the prompt (e.g. when the question changes between levels).
    pub fn set_prompt(&mut self, prompt: impl Into<String>) {
        self.prompt = prompt.into();
    }

    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
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

    /// Commit the current answer: return it and clear the line for the next
    /// entry. The prompt is left untouched. This is the hook a command layer
    /// consumes — the quiz example grades the returned text.
    pub fn submit(&mut self) -> String {
        let answer = self.line.text().to_string();
        self.line.clear();
        answer
    }

    /// Rasterise the prompt then the editable answer into `area`, clipped to it,
    /// with a caret after the answer's cursor. All glyph work goes through the
    /// shared [`overlay`](crate::overlay) text-run primitive.
    fn draw_text(&self, window: &mut Surface, area: Rect) {
        let px = self.config.font.size_px;
        let baseline = overlay::centered_baseline(area, self.font.line_metrics(px));

        // Fixed prompt first (no caret), then the answer on the same baseline;
        // draw_run returns the pen, so the answer starts where the prompt ended.
        let prompt_style = overlay::TextStyle::new(px, self.config.prompt_color);
        let text_style = overlay::TextStyle::new(px, self.config.text_color);
        let answer_start = overlay::draw_run(
            window,
            &self.font,
            area,
            &self.prompt,
            area.origin.x,
            baseline,
            prompt_style,
        );
        overlay::draw_run(
            window,
            &self.font,
            area,
            self.line.text(),
            answer_start,
            baseline,
            text_style,
        );

        // The caret sits after the cursor: the advance width of the answer text
        // before it, clamped inside the text area.
        let prefix = &self.line.text()[..self.line.cursor()];
        let caret_x = answer_start + overlay::advance_width(&self.font, prefix, px);
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
    fn caret_moves_across_utf8_and_forward_deletes() {
        let mut line = InputLine::new();
        for c in "aé".chars() {
            line.insert(c);
        }
        assert_eq!(line.cursor(), 3); // 'a' + 2-byte 'é'
        assert!(line.left()); // step over 'é'
        assert_eq!(line.cursor(), 1);
        assert!(line.left()); // step over 'a'
        assert_eq!(line.cursor(), 0);
        assert!(!line.left()); // already at start
        assert!(line.right()); // back to after 'a'
        assert!(line.delete()); // remove 'é' at the caret
        assert_eq!(line.text(), "a");
        assert!(!line.delete()); // nothing at the caret
    }

    #[test]
    fn home_end_and_handle_route_ui_input() {
        let mut line = InputLine::new();
        for c in "hi".chars() {
            line.insert(c);
        }
        assert!(line.home());
        assert_eq!(line.cursor(), 0);
        assert!(!line.home()); // already home
        assert!(line.end());
        assert_eq!(line.cursor(), 2);

        assert!(line.handle(UiInput::Char('!')));
        assert_eq!(line.text(), "hi!");
        assert!(line.handle(UiInput::Backspace));
        assert_eq!(line.text(), "hi");
        assert!(!line.handle(UiInput::Confirm)); // commit is ignored by the field
        assert!(!line.handle(UiInput::Char('\n'))); // control chars are ignored
    }

    #[test]
    fn clear_resets_buffer_and_cursor() {
        let mut line = InputLine::new();
        line.insert('x');
        line.clear();
        assert_eq!(line.text(), "");
        assert_eq!(line.cursor(), 0);
    }

    #[test]
    fn submit_returns_the_answer_and_clears() {
        let mut line = InputLine::new();
        for ch in "12".chars() {
            line.insert(ch);
        }
        // Mirror InputField::submit's read-then-clear on the model it wraps.
        let answer = line.text().to_string();
        line.clear();
        assert_eq!(answer, "12");
        assert_eq!(line.text(), "");
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn prompt_round_trips_through_the_field() {
        let font = SystemFont::load(&InputConfig::default().font).expect("a system font");
        let mut field = InputField::new(InputConfig::default(), font).with_prompt("Q: ");
        assert_eq!(field.prompt(), "Q: ");
        field.set_prompt("What is 6+6? ");
        assert_eq!(field.prompt(), "What is 6+6? ");
        // submit returns the typed answer and leaves the prompt intact.
        field.type_char('1');
        field.type_char('2');
        assert_eq!(field.submit(), "12");
        assert_eq!(field.prompt(), "What is 6+6? ");
        assert_eq!(field.line().text(), "");
    }
}
