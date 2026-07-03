//! A framed panel: fill + concentric border + a content rect.
//!
//! This is the reusable generalisation of the layout the input field computes
//! privately ([`InputConfig::layout`](crate::config::InputConfig::layout)): a
//! background fill, one or more nested border lines, and the inset rect that
//! remains for content. It is space-agnostic — it draws rects into any
//! [`Surface`], so it works in the pixel-art virtual screen or in device space.

use crate::color::Color;
use crate::config::BorderConfig;
use crate::geometry::{Point, Rect, Size};
use crate::surface::Surface;

/// Which edges of a [`Panel`]'s border lines to draw — a bitmask like Ratatui's
/// `Borders`. Combine with `|` (e.g. `Borders::TOP | Borders::BOTTOM`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Borders(u8);

impl Borders {
    /// Draw no edges.
    pub const NONE: Borders = Borders(0);
    /// The top edge.
    pub const TOP: Borders = Borders(1);
    /// The bottom edge.
    pub const BOTTOM: Borders = Borders(2);
    /// The left edge.
    pub const LEFT: Borders = Borders(4);
    /// The right edge.
    pub const RIGHT: Borders = Borders(8);
    /// All four edges (the default).
    pub const ALL: Borders = Borders(1 | 2 | 4 | 8);

    /// Whether every edge in `other` is set.
    #[must_use]
    pub fn contains(self, other: Borders) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Borders {
    type Output = Borders;
    fn bitor(self, rhs: Borders) -> Borders {
        Borders(self.0 | rhs.0)
    }
}

/// A filled, bordered frame around a content area. Built with a rect and refined
/// with builder methods; `fill` defaults to transparent (draw nothing) and the
/// border to all four edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    rect: Rect,
    fill: Option<Color>,
    border: BorderConfig,
    /// Which edges of each border line to stroke.
    borders: Borders,
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
            borders: Borders::ALL,
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

    /// Stroke only these edges of each border line (default [`Borders::ALL`]).
    #[must_use]
    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
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
        let border_extent = self.border.line_count * step;
        let drop_trailing_gap = if self.border.line_count > 0 {
            self.border.line_gap_px
        } else {
            0
        };
        let inset = (self.margin + border_extent).saturating_sub(drop_trailing_gap) + self.padding;
        self.rect.inset(inset)
    }

    /// A `height`-tall strip along the top edge, content-width wide, for a title.
    /// `Panel` stays font-free — draw a [`Label`](super::Label) into this rect.
    #[must_use]
    pub fn title_rect(&self, height: u32) -> Rect {
        let content = self.content_rect();
        Rect::new(
            Point::new(content.origin.x, self.rect.origin.y + self.margin as i32),
            Size::new(content.size.w, height),
        )
    }

    /// Fill (if set) then stroke the selected edges of every border line.
    pub fn draw(&self, surface: &mut Surface) {
        if let Some(fill) = self.fill {
            surface.fill_rect(self.rect, fill);
        }
        for line in self.border_rects() {
            draw_edges(
                surface,
                line,
                self.border.color,
                self.border.line_thickness_px,
                self.borders,
            );
        }
    }
}

/// Stroke the selected `borders` edges of `rect` as `thickness`-thick strips —
/// the per-edge form of [`Surface::draw_rect_outline`], matching it when all
/// edges are set.
fn draw_edges(surface: &mut Surface, rect: Rect, color: Color, thickness: u32, borders: Borders) {
    let t = thickness as i32;
    if borders.contains(Borders::TOP) {
        surface.fill_rect(
            Rect::new(rect.origin, Size::new(rect.size.w, thickness)),
            color,
        );
    }
    if borders.contains(Borders::BOTTOM) {
        surface.fill_rect(
            Rect::new(
                Point::new(rect.origin.x, rect.bottom() - t),
                Size::new(rect.size.w, thickness),
            ),
            color,
        );
    }
    if borders.contains(Borders::LEFT) {
        surface.fill_rect(
            Rect::new(rect.origin, Size::new(thickness, rect.size.h)),
            color,
        );
    }
    if borders.contains(Borders::RIGHT) {
        surface.fill_rect(
            Rect::new(
                Point::new(rect.right() - t, rect.origin.y),
                Size::new(thickness, rect.size.h),
            ),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn borders_combine_and_report_membership() {
        let b = Borders::TOP | Borders::BOTTOM;
        assert!(b.contains(Borders::TOP));
        assert!(b.contains(Borders::BOTTOM));
        assert!(!b.contains(Borders::LEFT));
        assert!(Borders::ALL.contains(Borders::LEFT | Borders::RIGHT));
        assert!(!Borders::NONE.contains(Borders::TOP));
    }

    #[test]
    fn all_borders_stroke_all_four_edges() {
        let stroke = Color::rgb(200, 100, 50);
        let panel = Panel::new(Rect::from_size(Size::new(10, 10))).border(border(1, 1, 0));
        let mut s = Surface::new(Size::new(10, 10), Color::rgb(0, 0, 0));
        panel.draw(&mut s);
        let at = |x: usize, y: usize| s.as_slice()[y * 10 + x];
        assert_eq!(at(5, 0), stroke.packed(), "top");
        assert_eq!(at(5, 9), stroke.packed(), "bottom");
        assert_eq!(at(0, 5), stroke.packed(), "left");
        assert_eq!(at(9, 5), stroke.packed(), "right");
    }

    #[test]
    fn partial_borders_stroke_only_selected_edges() {
        let stroke = Color::rgb(200, 100, 50);
        let panel = Panel::new(Rect::from_size(Size::new(10, 10)))
            .border(border(1, 1, 0))
            .borders(Borders::BOTTOM);
        let mut s = Surface::new(Size::new(10, 10), Color::rgb(0, 0, 0));
        panel.draw(&mut s);
        let at = |x: usize, y: usize| s.as_slice()[y * 10 + x];
        assert_eq!(at(5, 9), stroke.packed(), "bottom is drawn");
        assert_ne!(at(5, 0), stroke.packed(), "top is not drawn");
        assert_ne!(at(0, 5), stroke.packed(), "left is not drawn");
    }

    #[test]
    fn title_rect_is_a_top_strip_of_content_width() {
        let panel = Panel::new(Rect::from_size(Size::new(100, 50)))
            .border(border(2, 1, 0))
            .margin(4)
            .padding(2);
        let content = panel.content_rect();
        let title = panel.title_rect(10);
        assert_eq!(title.origin.x, content.origin.x);
        assert_eq!(title.size.w, content.size.w);
        assert_eq!(title.size.h, 10);
        assert_eq!(title.origin.y, 4); // rect top (0) + margin (4)
    }
}
