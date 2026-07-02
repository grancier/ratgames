//! A single line of anti-aliased text, aligned within a rect.
//!
//! Device-space convenience over the [`overlay`](crate::overlay) run primitive:
//! it measures the run, aligns it horizontally, centres it on the vertical
//! baseline, and draws it clipped to the rect. Menus, HUD text, and prompts use
//! it rather than re-deriving pen positions.

use crate::font::SystemFont;
use crate::geometry::Rect;
use crate::overlay::{self, TextStyle};
use crate::surface::Surface;

/// Horizontal alignment of a [`Label`] within its rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Left edge of the rect.
    Start,
    /// Horizontally centred.
    Center,
    /// Right edge of the rect.
    End,
}

/// A line of text plus its style and alignment. Borrows the text; drawing
/// supplies the font and target rect.
#[derive(Debug, Clone, Copy)]
pub struct Label<'a> {
    text: &'a str,
    style: TextStyle,
    align: Align,
}

impl<'a> Label<'a> {
    /// A left-aligned label of `text` in `style`.
    #[must_use]
    pub fn new(text: &'a str, style: TextStyle) -> Self {
        Self {
            text,
            style,
            align: Align::Start,
        }
    }

    /// Set the horizontal alignment.
    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Draw into `area` (clipped to it), horizontally aligned and vertically
    /// centred on the baseline. Returns the pen x after the run.
    pub fn draw(&self, surface: &mut Surface, font: &SystemFont, area: Rect) -> i32 {
        let width = overlay::advance_width(font, self.text, self.style.size_px);
        let start = aligned_start(area, width, self.align);
        let baseline = overlay::centered_baseline(area, font.line_metrics(self.style.size_px));
        overlay::draw_run(surface, font, area, self.text, start, baseline, self.style)
    }
}

/// The starting pen x for a run of `text_width` px aligned within `area`.
fn aligned_start(area: Rect, text_width: i32, align: Align) -> i32 {
    match align {
        Align::Start => area.origin.x,
        Align::Center => area.origin.x + (area.size.w as i32 - text_width) / 2,
        Align::End => area.right() - text_width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::geometry::{Point, Size};

    #[test]
    fn alignment_positions_the_run_within_the_area() {
        let area = Rect::new(Point::new(10, 0), Size::new(100, 20));
        assert_eq!(aligned_start(area, 40, Align::Start), 10);
        assert_eq!(aligned_start(area, 40, Align::Center), 40); // 10 + (100-40)/2
        assert_eq!(aligned_start(area, 40, Align::End), 70); // right(110) - 40
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn draw_blends_and_returns_the_advanced_pen() {
        use crate::config::FontConfig;
        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(120, 30), bg);
        let area = Rect::from_size(s.size());
        let end = Label::new("Hi", TextStyle::new(18.0, Color::rgb(255, 255, 255)))
            .align(Align::Center)
            .draw(&mut s, &font, area);
        assert!(end > area.origin.x, "pen advanced");
        assert!(
            s.as_slice().iter().any(|&w| w != bg.packed()),
            "some pixel was blended"
        );
    }
}
