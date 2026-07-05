//! [`ChoiceList`] — a pixel-art selection list: a [`Menu`] rendered as a vertical
//! stack of [`ShadowBanner`]s, the highlight marked with a leading caret.
//!
//! The banner-backed counterpart to [`MenuView`](super::MenuView): where a
//! `MenuView` borrows a menu and draws anti-aliased [`Label`](super::Label)s live
//! each frame, a `ChoiceList` *owns* its menu and *pre-bakes* one `ShadowBanner`
//! per option — baking pixel-art text is expensive, so it re-bakes only when the
//! highlight moves. It marks the selection with a caret glyph rather than a colour,
//! so it reads on a chunky 8-bit palette.
//!
//! The list is anchored, laid out, and magnified by the caller (product values it
//! passes in); the caret + vertical stack + re-bake-on-move mechanic is the
//! reusable part. It composites through a [`ShadowBannerFactory`], which borrows
//! its glyph source and so is short-lived — the caller passes a fresh one to
//! [`new`](ChoiceList::new) and [`handle`](ChoiceList::handle) rather than the list
//! storing it.

use super::{Menu, ShadowBanner, ShadowBannerFactory, UiInput};
use crate::geometry::{Point, Rect};
use crate::present::OverlayLayer;
use crate::surface::Surface;

/// A pixel-art choice list: an owned [`Menu`] plus one baked [`ShadowBanner`] per
/// option, stacked from an anchor point and re-baked when the highlight moves.
/// Implements [`OverlayLayer`], so a caller pushes the whole list as one overlay.
pub struct ChoiceList {
    menu: Menu,
    banners: Vec<ShadowBanner>,
    origin: Point,
    row_pitch: i32,
    scale: u32,
}

impl ChoiceList {
    /// A list over `labels`, its rows stacked down from `origin` every `row_pitch`
    /// virtual pixels, each banner magnified by `scale` and baked through
    /// `factory`. The first option starts highlighted.
    #[must_use]
    pub fn new<I, S>(
        labels: I,
        origin: Point,
        row_pitch: i32,
        scale: u32,
        factory: &ShadowBannerFactory,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let menu = Menu::new(labels);
        let banners = bake(&menu, origin, row_pitch, scale, factory);
        Self {
            menu,
            banners,
            origin,
            row_pitch,
            scale,
        }
    }

    /// The highlighted index (`0` when empty).
    #[must_use]
    pub fn selected(&self) -> usize {
        self.menu.selected()
    }

    /// The highlighted label, or `None` when empty.
    #[must_use]
    pub fn selected_label(&self) -> Option<&str> {
        self.menu.selected_item()
    }

    /// Number of options.
    #[must_use]
    pub fn len(&self) -> usize {
        self.menu.len()
    }

    /// Whether there are no options.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.menu.is_empty()
    }

    /// Apply a navigation / confirm command (via [`Menu::handle`]), re-baking the
    /// banners through `factory` only if the highlight actually moved. Returns the
    /// chosen index on `Confirm`, `None` on navigation — exactly like the menu.
    pub fn handle(&mut self, input: UiInput, factory: &ShadowBannerFactory) -> Option<usize> {
        let before = self.menu.selected();
        let chosen = self.menu.handle(input);
        if self.menu.selected() != before {
            self.banners = bake(&self.menu, self.origin, self.row_pitch, self.scale, factory);
        }
        chosen
    }
}

/// Bake `menu`'s options as a left-anchored vertical stack of banners: the
/// highlighted row prefixed with a `"> "` caret, the rest padded with `"  "` so
/// their labels align under it.
fn bake(
    menu: &Menu,
    origin: Point,
    row_pitch: i32,
    scale: u32,
    factory: &ShadowBannerFactory,
) -> Vec<ShadowBanner> {
    menu.items()
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let text = if i == menu.selected() {
                format!("> {label}")
            } else {
                format!("  {label}")
            };
            let at = Point::new(origin.x, origin.y + i as i32 * row_pitch);
            factory.at(&text, at, scale)
        })
        .collect()
}

impl OverlayLayer for ChoiceList {
    fn render(&self, window: &mut Surface, viewport: Rect) {
        for banner in &self.banners {
            banner.render(window, viewport);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::geometry::Size;
    use crate::glyph::Bitmap8x8;
    use crate::ui::ShadowStyle;

    /// A factory over the chunky bitmap font (no system font needed, so these tests
    /// are not `#[ignore]`d), sized to the given virtual screen.
    fn list(factory: &ShadowBannerFactory) -> ChoiceList {
        ChoiceList::new(["3", "4", "5"], Point::new(2, 2), 10, 1, factory)
    }

    #[test]
    fn bakes_one_row_per_option_starting_on_the_first() {
        let src = Bitmap8x8;
        let f = ShadowBannerFactory::new(&src, ShadowStyle::default(), Size::new(64, 64));
        let cl = list(&f);
        assert_eq!(cl.len(), 3);
        assert!(!cl.is_empty());
        assert_eq!(cl.selected(), 0);
        assert_eq!(cl.selected_label(), Some("3"));
    }

    #[test]
    fn navigation_moves_the_highlight_and_confirm_returns_it() {
        let src = Bitmap8x8;
        let f = ShadowBannerFactory::new(&src, ShadowStyle::default(), Size::new(64, 64));
        let mut cl = list(&f);
        assert_eq!(cl.handle(UiInput::Down, &f), None);
        assert_eq!(cl.selected(), 1);
        assert_eq!(cl.selected_label(), Some("4"));
        assert_eq!(cl.handle(UiInput::Confirm, &f), Some(1));
    }

    #[test]
    fn rendering_draws_pixels_and_the_caret_follows_the_highlight() {
        let src = Bitmap8x8;
        let vs = Size::new(64, 64);
        let f = ShadowBannerFactory::new(&src, ShadowStyle::default(), vs);
        let mut cl = list(&f);
        let vp = Rect::from_size(vs);
        let bg = Color::rgb(0, 0, 0);
        let snapshot = |cl: &ChoiceList| {
            let mut w = Surface::new(vs, bg);
            cl.render(&mut w, vp);
            w.as_slice().to_vec()
        };

        let first = snapshot(&cl);
        assert!(
            first.iter().any(|&p| p != bg.packed()),
            "the list draws something"
        );

        cl.handle(UiInput::Down, &f);
        let second = snapshot(&cl);
        assert_ne!(first, second, "moving the caret changes the pixels");
    }

    #[test]
    fn an_empty_list_bakes_no_rows() {
        let src = Bitmap8x8;
        let f = ShadowBannerFactory::new(&src, ShadowStyle::default(), Size::new(64, 64));
        let cl = ChoiceList::new(Vec::<String>::new(), Point::new(2, 2), 10, 1, &f);
        assert!(cl.is_empty());
        assert_eq!(cl.len(), 0);
        assert_eq!(cl.selected_label(), None);
    }
}
