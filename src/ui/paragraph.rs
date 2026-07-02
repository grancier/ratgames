//! Multi-line, word-wrapped, aligned anti-aliased text — the prompt / lesson
//! text primitive.
//!
//! Ratatui's `Paragraph` in our pixel world: [`wrap_lines`] breaks text to a
//! width (honouring explicit newlines), and [`Paragraph::draw`] lays the lines
//! down a rect through the [`overlay`](crate::overlay) run primitive, each line
//! aligned via [`Align`]. Single-line text uses [`Label`](super::Label); this is
//! for anything that must wrap.

use super::Align;
use crate::font::SystemFont;
use crate::geometry::Rect;
use crate::overlay::{self, TextStyle};
use crate::surface::Surface;

/// A block of text plus its style and alignment. Drawing supplies the font and
/// rect and wraps to the rect's width.
#[derive(Debug, Clone, Copy)]
pub struct Paragraph<'a> {
    text: &'a str,
    style: TextStyle,
    align: Align,
}

impl<'a> Paragraph<'a> {
    /// A left-aligned paragraph of `text` in `style`.
    #[must_use]
    pub fn new(text: &'a str, style: TextStyle) -> Self {
        Self {
            text,
            style,
            align: Align::Start,
        }
    }

    /// Set the horizontal alignment of each line.
    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Wrap to `area`'s width and draw top-down, each line aligned and clipped to
    /// `area`. Returns the number of lines drawn (lines below the area are
    /// skipped).
    pub fn draw(&self, surface: &mut Surface, font: &SystemFont, area: Rect) -> usize {
        let metrics = font.line_metrics(self.style.size_px);
        let line_h = (metrics.line_height.round() as i32).max(1);
        let ascent = metrics.ascent.round() as i32;
        let lines = wrap_lines(self.text, area.size.w as i32, |s| {
            overlay::advance_width(font, s, self.style.size_px)
        });

        let mut drawn = 0;
        let mut top = area.origin.y;
        for line in &lines {
            if top >= area.bottom() {
                break;
            }
            let width = overlay::advance_width(font, line, self.style.size_px);
            let x = self.align.start_x(area, width);
            overlay::draw_run(surface, font, area, line, x, top + ascent, self.style);
            drawn += 1;
            top += line_h;
        }
        drawn
    }
}

/// Break `text` into lines no wider than `max_width`, measured by `measure`.
/// Explicit `\n`s always break (blank lines preserved); within a line, wrapping
/// is greedy on whitespace, and a single word wider than `max_width` is left on
/// its own (overflowing) line rather than looping forever.
pub fn wrap_lines(text: &str, max_width: i32, measure: impl Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || measure(&candidate) <= max_width {
                current = candidate;
            } else {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            }
        }
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic measure: one unit per character (spaces included).
    fn chars(s: &str) -> i32 {
        s.chars().count() as i32
    }

    #[test]
    fn wraps_greedily_on_word_boundaries() {
        assert_eq!(
            wrap_lines("the quick brown fox", 9, chars),
            vec!["the quick", "brown fox"]
        );
    }

    #[test]
    fn preserves_explicit_and_blank_lines() {
        assert_eq!(wrap_lines("a\n\nb", 10, chars), vec!["a", "", "b"]);
    }

    #[test]
    fn a_word_wider_than_the_line_gets_its_own_line() {
        assert_eq!(
            wrap_lines("hi supercalifragilistic ok", 5, chars),
            vec!["hi", "supercalifragilistic", "ok"]
        );
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn draw_wraps_long_text_to_multiple_lines() {
        use crate::color::Color;
        use crate::config::FontConfig;
        use crate::geometry::Size;

        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(120, 200), bg);
        let area = Rect::from_size(s.size());
        let drawn = Paragraph::new(
            "The quick brown fox jumps over the lazy dog",
            TextStyle::new(16.0, Color::rgb(255, 255, 255)),
        )
        .align(Align::Center)
        .draw(&mut s, &font, area);
        assert!(drawn >= 2, "long text should wrap to multiple lines");
        assert!(s.as_slice().iter().any(|&w| w != bg.packed()));
    }
}
