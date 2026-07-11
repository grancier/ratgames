//! [`ChoiceScreen`] — a reusable menu screen: a title banner over a [`ChoiceList`],
//! routing the chosen option (or a cancel) onward.
//!
//! The arcade "pick one" screen — a difficulty select, a mode select, a pause
//! menu. It owns a title [`ShadowBanner`] and a [`ChoiceList`] and *is* a
//! [`Screen`], so a game drops it onto the [`ScreenStack`](crate::ScreenStack)
//! without a per-screen boilerplate `impl`: arrows navigate, confirm chooses,
//! cancel backs out. Where a choice or a cancel *leads* is the game's business —
//! the screen holds two one-shot transitions, guarded by [`Option::take`] like a
//! [`TimedCard`](super::TimedCard), run with a fresh `&mut Ctx` at the moment they
//! fire.
//!
//! The one wrinkle over a `TimedCard`: a `ChoiceList` re-bakes its caret when the
//! highlight moves, and baking pixel-art text needs the game's glyph source —
//! which lives in the game's context, not the toolkit. So `Ctx` must implement
//! [`BannerContext`], handing the screen the game's [`ShadowBannerFactory`] on
//! demand. The list's labels, layout, and scale are product values the game bakes
//! in when it builds the [`ChoiceList`]; the caret + navigation + routing are the
//! reusable part.

use super::{Screen, ScreenChange};
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::{ChoiceList, ShadowBanner, ShadowBannerFactory, UiInput};

/// A screen context that can produce the game's pixel-art [`ShadowBannerFactory`]
/// — its glyph source, shadow style, and virtual screen — so a reusable pixel-art
/// screen can re-bake its banners in the game's own look without the toolkit
/// knowing any of it. A game implements this once on its screen context; screens
/// like [`ChoiceScreen`] that re-bake on interaction require it.
pub trait BannerContext {
    /// The game's banner factory, in its configured style.
    fn banner_factory(&self) -> ShadowBannerFactory<'_>;
}

/// The one-shot transition a [`ChoiceScreen`] fires on a confirmed choice: given
/// the chosen index and mutable session state, it returns the stack change.
type OnSelect<Ctx> = Box<dyn FnOnce(usize, &mut Ctx) -> ScreenChange<Ctx>>;

/// The one-shot transition a [`ChoiceScreen`] fires on cancel.
type OnCancel<Ctx> = Box<dyn FnOnce(&mut Ctx) -> ScreenChange<Ctx>>;

/// A menu screen: a title [`ShadowBanner`] above a [`ChoiceList`], with arrows to
/// navigate, confirm to choose, and cancel to back out. Construct with
/// [`new`](Self::new) and push onto the [`ScreenStack`](crate::ScreenStack); the
/// game supplies the title, the pre-built list (its labels / layout / scale are
/// the game's product values), and where a choice or a cancel leads.
///
/// Confirm routes the chosen index through `on_select`; cancel routes through
/// `on_cancel`. Each fires at most once ([`Option::take`]). Navigation re-bakes
/// the list's caret through the game's [`BannerContext`], so `Ctx` must implement
/// it for the screen to be a [`Screen`].
pub struct ChoiceScreen<Ctx> {
    title: ShadowBanner,
    choices: ChoiceList,
    on_select: Option<OnSelect<Ctx>>,
    on_cancel: Option<OnCancel<Ctx>>,
}

impl<Ctx> ChoiceScreen<Ctx> {
    /// A menu titled by `title` over `choices`, routing a confirmed index through
    /// `on_select` and a cancel through `on_cancel`.
    #[must_use]
    pub fn new(
        title: ShadowBanner,
        choices: ChoiceList,
        on_select: impl FnOnce(usize, &mut Ctx) -> ScreenChange<Ctx> + 'static,
        on_cancel: impl FnOnce(&mut Ctx) -> ScreenChange<Ctx> + 'static,
    ) -> Self {
        Self {
            title,
            choices,
            on_select: Some(Box::new(on_select)),
            on_cancel: Some(Box::new(on_cancel)),
        }
    }

    /// The currently highlighted option index (`0` when the list is empty).
    #[must_use]
    pub fn highlighted(&self) -> usize {
        self.choices.selected()
    }

    /// Fire the one-shot select transition, or [`ScreenChange::None`] if it (or the
    /// cancel) has already fired.
    fn select(&mut self, index: usize, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match self.on_select.take() {
            Some(on_select) => on_select(index, ctx),
            None => ScreenChange::None,
        }
    }

    /// Fire the one-shot cancel transition, or [`ScreenChange::None`] if it (or the
    /// select) has already fired.
    fn cancel(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match self.on_cancel.take() {
            Some(on_cancel) => on_cancel(ctx),
            None => ScreenChange::None,
        }
    }
}

impl<Ctx: BannerContext> Screen<Ctx> for ChoiceScreen<Ctx> {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Cancel => self.cancel(ctx),
            // Arrows navigate (re-baking the caret in the game's style); confirm
            // returns the highlighted index.
            other => {
                let chosen = {
                    let factory = ctx.banner_factory();
                    self.choices.handle(other, &factory)
                };
                match chosen {
                    Some(index) => self.select(index, ctx),
                    None => ScreenChange::None,
                }
            }
        }
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(&self.title);
        overlays.push(&self.choices);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::glyph::Bitmap8x8;
    use crate::ui::ShadowStyle;

    /// A test context that owns a glyph source (so it can hand out a factory) and
    /// records what the screen routed.
    struct Ctx {
        src: Bitmap8x8,
        picked: Option<usize>,
        cancelled: bool,
    }

    impl Ctx {
        fn new() -> Self {
            Self {
                src: Bitmap8x8,
                picked: None,
                cancelled: false,
            }
        }
    }

    impl BannerContext for Ctx {
        fn banner_factory(&self) -> ShadowBannerFactory<'_> {
            ShadowBannerFactory::new(&self.src, ShadowStyle::default(), Size::new(64, 64))
        }
    }

    /// A menu over `labels` that records the chosen index / a cancel into the ctx.
    fn menu<const N: usize>(ctx: &Ctx, labels: [&str; N]) -> ChoiceScreen<Ctx> {
        let factory = ctx.banner_factory();
        let title = factory.at("MENU", Point::new(0, 0), 1);
        let choices = ChoiceList::new(labels, Point::new(0, 10), 10, 1, &factory);
        ChoiceScreen::new(
            title,
            choices,
            |index, ctx: &mut Ctx| {
                ctx.picked = Some(index);
                ScreenChange::None
            },
            |ctx: &mut Ctx| {
                ctx.cancelled = true;
                ScreenChange::None
            },
        )
    }

    #[test]
    fn confirm_routes_the_highlighted_choice() {
        let mut ctx = Ctx::new();
        let mut screen = menu(&ctx, ["EASY", "NORMAL", "HARD"]);
        screen.handle(UiInput::Down, &mut ctx); // highlight NORMAL (index 1)
        assert_eq!(screen.highlighted(), 1);

        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.picked, Some(1), "confirm routes the highlighted index");
        assert!(!ctx.cancelled);
    }

    #[test]
    fn cancel_routes_through_on_cancel() {
        let mut ctx = Ctx::new();
        let mut screen = menu(&ctx, ["EASY", "NORMAL"]);
        screen.handle(UiInput::Cancel, &mut ctx);
        assert!(ctx.cancelled);
        assert_eq!(ctx.picked, None, "cancel is not a choice");
    }

    #[test]
    fn navigation_moves_the_highlight_without_choosing() {
        let mut ctx = Ctx::new();
        let mut screen = menu(&ctx, ["EASY", "NORMAL", "HARD"]);
        assert_eq!(screen.highlighted(), 0);
        screen.handle(UiInput::Down, &mut ctx);
        screen.handle(UiInput::Down, &mut ctx);
        assert_eq!(screen.highlighted(), 2);
        assert_eq!(ctx.picked, None, "navigating chooses nothing");
        assert!(!ctx.cancelled);
    }

    #[test]
    fn a_choice_fires_at_most_once() {
        // Confirm chooses; a second confirm (the screen would normally have left)
        // must not re-fire the one-shot.
        let mut ctx = Ctx::new();
        let mut screen = menu(&ctx, ["EASY", "NORMAL"]);
        screen.handle(UiInput::Confirm, &mut ctx);
        ctx.picked = None; // pretend the caller cleared it
        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.picked, None, "the choice fired once");
    }

    #[test]
    fn collect_layers_contributes_the_title_and_the_list() {
        let ctx = Ctx::new();
        let screen = menu(&ctx, ["EASY", "NORMAL"]);
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        screen.collect_layers(&ctx, &mut world, &mut overlays);
        // The title banner plus the choice list (one overlay for the whole list).
        assert_eq!(overlays.len(), 2);
        assert!(world.is_empty(), "a menu is device-space overlays only");
    }
}
