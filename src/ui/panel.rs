//! A framed panel: fill + concentric border + a content rect.
//!
//! This is the reusable generalisation of the layout the input field computes
//! privately ([`InputConfig::layout`](crate::config::InputConfig::layout)): a
//! background fill, one or more nested border lines, and the inset rect that
//! remains for content. It is space-agnostic — it draws rects into any
//! [`Surface`], so it works in the pixel-art virtual screen or in device space.

use crate::color::Color;
use crate::config::BorderConfig;
use crate::geometry::Rect;
use crate::surface::Surface;

/// A filled, bordered frame around a content area. Built with a rect and refined
/// with builder methods; `fill` defaults to transparent (draw nothing).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    rect: Rect,
    fill: Option<Color>,
    border: BorderConfig,
    /// Gap from the panel edge to the outermost border line, in pixels.
    margin: u32,
    /// Gap from the innermost border line to the content rect, in pixels.
    padding: u32,
}

impl Panel {
    /// A panel filling `rect`, with the default border and no fill.
    #[must_use]
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            fill: None,
            border: BorderConfig::default(),
            margin: 0,
            padding: 0,
        }
    }

    /// Paint the panel background this colour before the borders.
    #[must_use]
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Use this border style (colour, thickness, line count, gap).
    #[must_use]
    pub fn border(mut self, border: BorderConfig) -> Self {
        self.border = border;
        self
    }

    /// Drop the border entirely (a plain filled/`content` panel).
    #[must_use]
    pub fn borderless(mut self) -> Self {
        self.border.line_count = 0;
        self
    }

    /// Gap from the panel edge to the outermost border, in pixels.
    #[must_use]
    pub fn margin(mut self, px: u32) -> Self {
        self.margin = px;
        self
    }

    /// Gap from the innermost border to the content rect, in pixels.
    #[must_use]
    pub fn padding(mut self, px: u32) -> Self {
        self.padding = px;
        self
    }

    /// The panel's outer rect.
    #[must_use]
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// The concentric border rects, outermost first (empty when borderless).
    #[must_use]
    pub fn border_rects(&self) -> Vec<Rect> {
        let step = self.border.line_thickness_px + self.border.line_gap_px;
        (0..self.border.line_count)
            .map(|i| self.rect.inset(self.margin + i * step))
            .collect()
    }

    /// The rect left for content: inset past the margin, every border line, and
    /// the padding. The trailing inter-line gap is dropped (there is no line
    /// after the last one) when there is at least one border.
    #[must_use]
    pub fn content_rect(&self) -> Rect {
        let step = self.border.line_thickness_px + self.border.line_gap_px;
        let borders = self.border.line_count * step;
        let drop_trailing_gap = if self.border.line_count > 0 {
            self.border.line_gap_px
        } else {
            0
        };
        let inset = (self.margin + borders).saturating_sub(drop_trailing_gap) + self.padding;
        self.rect.inset(inset)
    }

    /// Fill (if set) then stroke every border line into `surface`.
    pub fn draw(&self, surface: &mut Surface) {
        if let Some(fill) = self.fill {
            surface.fill_rect(self.rect, fill);
        }
        for line in self.border_rects() {
            surface.draw_rect_outline(line, self.border.color, self.border.line_thickness_px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;

    fn border(thickness: u32, count: u32, gap: u32) -> BorderConfig {
        BorderConfig {
            color: Color::rgb(200, 100, 50),
            line_thickness_px: thickness,
            line_count: count,
            line_gap_px: gap,
        }
    }

    #[test]
    fn content_rect_insets_past_margin_borders_and_padding() {
        // Mirrors InputConfig::layout: step = 2+3 = 5; inset = 8 + 2*5 - 3 + 8 = 23.
        let base = Rect::from_size(Size::new(200, 100));
        let panel = Panel::new(base)
            .border(border(2, 2, 3))
            .margin(8)
            .padding(8);
        assert_eq!(panel.content_rect(), base.inset(23));
    }

    #[test]
    fn border_rects_are_concentric_outermost_first() {
        let base = Rect::from_size(Size::new(100, 100));
        let panel = Panel::new(base).border(border(2, 3, 3)).margin(4);
        let rects = panel.border_rects();
        // step = 5: insets 4, 9, 14.
        assert_eq!(rects, vec![base.inset(4), base.inset(9), base.inset(14)]);
    }

    #[test]
    fn borderless_drops_lines_and_content_is_margin_plus_padding() {
        let base = Rect::from_size(Size::new(50, 50));
        let panel = Panel::new(base).margin(4).padding(6).borderless();
        assert!(panel.border_rects().is_empty());
        assert_eq!(panel.content_rect(), base.inset(10));
    }

    #[test]
    fn draw_paints_fill_then_border() {
        let fill = Color::rgb(10, 20, 30);
        let stroke = Color::rgb(200, 100, 50);
        let panel = Panel::new(Rect::from_size(Size::new(20, 20)))
            .fill(fill)
            .border(border(1, 1, 0))
            .margin(2);
        let mut s = Surface::new(Size::new(20, 20), Color::rgb(0, 0, 0));
        panel.draw(&mut s);
        let has = |c: Color| s.as_slice().iter().any(|&w| w == c.packed());
        assert!(has(fill), "fill should be painted");
        assert!(has(stroke), "border should be stroked");
    }
}
