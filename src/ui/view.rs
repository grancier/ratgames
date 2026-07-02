//! Rendering a [`Menu`] as an anti-aliased overlay.
//!
//! [`MenuView`] draws a menu's items as a vertical stack of centred [`Label`]s,
//! styling the highlighted item differently. The vertical layout is the pure,
//! testable [`stacked_rects`]; the text draw goes through [`Label`] (device
//! space), so a menu is readable rather than pixel-chunky.

use super::{Align, Label, Menu};
use crate::font::SystemFont;
use crate::geometry::{Point, Rect, Size};
use crate::overlay::TextStyle;
use crate::present::OverlayLayer;
use crate::surface::Surface;

/// Lay out `count` full-width rows of `item_height`, separated by `gap`, from the
/// top of `area` downward. The shared vertical-list layout (menus, HUD stacks).
#[must_use]
pub fn stacked_rects(area: Rect, count: usize, item_height: u32, gap: u32) -> Vec<Rect> {
    let stride = item_height as i32 + gap as i32;
    (0..count)
        .map(|i| {
            Rect::new(
                Point::new(area.origin.x, area.origin.y + i as i32 * stride),
                Size::new(area.size.w, item_height),
            )
        })
        .collect()
}

/// Renders a [`Menu`] as a vertical list of centred labels, highlighting the
/// selected item with a distinct [`TextStyle`]. Borrows the menu and font, so it
/// reflects the live selection each frame.
#[derive(Debug)]
pub struct MenuView<'a> {
    menu: &'a Menu,
    font: &'a SystemFont,
    area: Rect,
    item_height: u32,
    gap: u32,
    normal: TextStyle,
    selected: TextStyle,
}

impl<'a> MenuView<'a> {
    /// Draw `menu` into `area` using `normal` for items and `selected` for the
    /// highlighted one. Row height defaults to the normal text size; refine with
    /// [`item_height`](Self::item_height) / [`gap`](Self::gap).
    #[must_use]
    pub fn new(
        menu: &'a Menu,
        font: &'a SystemFont,
        area: Rect,
        normal: TextStyle,
        selected: TextStyle,
    ) -> Self {
        Self {
            menu,
            font,
            area,
            item_height: (normal.size_px.ceil() as u32).max(1),
            gap: 4,
            normal,
            selected,
        }
    }

    /// Set the per-row height, in device pixels.
    #[must_use]
    pub fn item_height(mut self, px: u32) -> Self {
        self.item_height = px.max(1);
        self
    }

    /// Set the gap between rows, in device pixels.
    #[must_use]
    pub fn gap(mut self, px: u32) -> Self {
        self.gap = px;
        self
    }

    /// The rect each item occupies, top to bottom.
    #[must_use]
    pub fn item_rects(&self) -> Vec<Rect> {
        stacked_rects(self.area, self.menu.len(), self.item_height, self.gap)
    }
}

impl OverlayLayer for MenuView<'_> {
    fn render(&self, window: &mut Surface, _viewport: Rect) {
        for (i, rect) in self.item_rects().into_iter().enumerate() {
            let style = if i == self.menu.selected() {
                self.selected
            } else {
                self.normal
            };
            Label::new(&self.menu.items()[i], style)
                .align(Align::Center)
                .draw(window, self.font, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_rects_lay_out_a_vertical_list() {
        let area = Rect::new(Point::new(5, 10), Size::new(80, 200));
        let rects = stacked_rects(area, 3, 20, 4);
        assert_eq!(
            rects,
            vec![
                Rect::new(Point::new(5, 10), Size::new(80, 20)),
                Rect::new(Point::new(5, 34), Size::new(80, 20)), // 10 + (20+4)
                Rect::new(Point::new(5, 58), Size::new(80, 20)),
            ]
        );
    }

    #[test]
    fn stacked_rects_empty_when_count_is_zero() {
        let area = Rect::from_size(Size::new(50, 50));
        assert!(stacked_rects(area, 0, 10, 2).is_empty());
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn menu_view_draws_each_item() {
        use crate::color::Color;
        use crate::config::FontConfig;

        let font = SystemFont::load(&FontConfig::default()).expect("a system font");
        let menu = Menu::new(["Start", "Options", "Quit"]);
        let bg = Color::rgb(0, 0, 0);
        let mut window = Surface::new(Size::new(200, 200), bg);
        let view = MenuView::new(
            &menu,
            &font,
            Rect::from_size(Size::new(200, 200)),
            TextStyle::new(20.0, Color::rgb(180, 180, 180)),
            TextStyle::new(20.0, Color::rgb(255, 255, 0)),
        )
        .item_height(30)
        .gap(6);
        assert_eq!(view.item_rects().len(), 3);
        view.render(&mut window, Rect::from_size(Size::new(200, 200)));
        assert!(
            window.as_slice().iter().any(|&w| w != bg.packed()),
            "menu should draw some glyph pixels"
        );
    }
}
