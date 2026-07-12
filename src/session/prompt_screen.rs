//! [`PromptScreen`] — a reusable static-card screen: banners held until the
//! player confirms or cancels.
//!
//! The arcade "press start" family: a title splash, a result card, a high-score
//! board — owned pre-baked banners with nothing to re-bake on interaction, where
//! the player's only moves are [`Confirm`](UiInput::Confirm) and
//! [`Cancel`](UiInput::Cancel). It is [`TimedCard`](super::TimedCard) with no
//! clock: the same owned overlays and the same single one-shot [`FnOnce`]
//! transition mapping the exit reason ([`PromptExit`]) to a [`ScreenChange`]
//! with a fresh `&mut Ctx` — `ratgames` owns the waiting and the rendering
//! while the game owns the routing, and the transition fires **once** (guarded
//! by [`Option::take`]) so two exits cannot double-fire.
//!
//! [`with_idle`](PromptScreen::with_idle) arms an idle [`Countdown`]: any input
//! pushes it back to its full budget, and its unbroken expiry exits as
//! [`Idled`](PromptExit::Idled) — how an idle title slips into an attract loop
//! while an active player never sees it.

use super::{Screen, ScreenChange};
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::{Countdown, ShadowBanner, UiInput};

/// Why a [`PromptScreen`] is leaving: the player confirmed, the player
/// cancelled, or the idle trigger expired untouched. The screen's transition
/// maps this to a [`ScreenChange`], so one closure forks on the reason — start
/// the game, quit, slip into attract mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptExit {
    /// The player pressed [`Confirm`](UiInput::Confirm).
    Confirmed,
    /// The player pressed [`Cancel`](UiInput::Cancel).
    Cancelled,
    /// The idle trigger ([`with_idle`](PromptScreen::with_idle)) expired with
    /// no input to push it back.
    Idled,
}

/// The one-shot transition a [`PromptScreen`] fires as it leaves: given the
/// exit reason and mutable session state, it returns the stack change to apply.
type PromptTransition<Ctx> = Box<dyn FnOnce(PromptExit, &mut Ctx) -> ScreenChange<Ctx>>;

/// A static-card [`Screen`]: [`ShadowBanner`] overlays shown until the player
/// confirms or cancels (or an optional idle trigger expires), then a single
/// transition routes onward. Construct with [`new`](Self::new) and push onto
/// the [`ScreenStack`](crate::ScreenStack); the game supplies the banners and
/// where each exit leads. [`with_idle`](Self::with_idle) arms the idle trigger
/// (a title screen's attract handoff).
pub struct PromptScreen<Ctx = ()> {
    overlays: Vec<ShadowBanner>,
    idle: Option<Countdown>,
    on_exit: Option<PromptTransition<Ctx>>,
}

impl<Ctx> PromptScreen<Ctx> {
    /// A card showing `overlays` until the player confirms or cancels, routing
    /// each exit through `on_exit`. The transition fires at most once, with a
    /// fresh `&mut Ctx` — so it observes current session state and cannot
    /// double-fire.
    #[must_use]
    pub fn new(
        overlays: Vec<ShadowBanner>,
        on_exit: impl FnOnce(PromptExit, &mut Ctx) -> ScreenChange<Ctx> + 'static,
    ) -> Self {
        Self {
            overlays,
            idle: None,
            on_exit: Some(Box::new(on_exit)),
        }
    }

    /// Arm the idle trigger: any input pushes it back to its full budget, and
    /// its unbroken expiry exits as [`Idled`](PromptExit::Idled).
    #[must_use]
    pub fn with_idle(mut self, idle: Countdown) -> Self {
        self.idle = Some(idle);
        self
    }

    /// Fire the one-shot transition for `why`, or [`ScreenChange::None`] if it
    /// has already fired — so a confirm and an idle expiry cannot double-fire.
    fn exit(&mut self, why: PromptExit, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match self.on_exit.take() {
            Some(on_exit) => on_exit(why, ctx),
            None => ScreenChange::None,
        }
    }
}

impl<Ctx> Screen<Ctx> for PromptScreen<Ctx> {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // Any key is a sign of life: push the idle trigger back.
        if let Some(idle) = self.idle.as_mut() {
            idle.reset();
        }
        match input {
            UiInput::Confirm => self.exit(PromptExit::Confirmed, ctx),
            UiInput::Cancel => self.exit(PromptExit::Cancelled, ctx),
            _ => ScreenChange::None,
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let idled = self.idle.as_mut().is_some_and(|idle| {
            idle.advance();
            idle.is_expired()
        });
        if idled {
            self.exit(PromptExit::Idled, ctx)
        } else {
            ScreenChange::None
        }
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        for banner in &self.overlays {
            overlays.push(banner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::glyph::Bitmap8x8;
    use crate::ui::{ShadowBannerFactory, ShadowStyle};

    /// Records each exit reason the prompt fires, proving the one-shot
    /// transition runs with `&mut Ctx` and the right [`PromptExit`].
    #[derive(Default)]
    struct Ctx {
        exits: Vec<PromptExit>,
    }

    /// `n` throwaway banners (their content is irrelevant to the mechanic).
    fn banners(n: usize) -> Vec<ShadowBanner> {
        let factory =
            ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
        (0..n)
            .map(|i| factory.at("HI", Point::new(0, i as i32), 1))
            .collect()
    }

    /// A prompt of `n` banners that logs its exit reason.
    fn prompt(n: usize) -> PromptScreen<Ctx> {
        PromptScreen::new(banners(n), |why, ctx: &mut Ctx| {
            ctx.exits.push(why);
            ScreenChange::None
        })
    }

    #[test]
    fn confirm_exits_as_confirmed() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1);
        assert!(matches!(
            p.handle(UiInput::Confirm, &mut ctx),
            ScreenChange::None
        ));
        assert_eq!(ctx.exits, [PromptExit::Confirmed]);
    }

    #[test]
    fn cancel_exits_as_cancelled() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1);
        p.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [PromptExit::Cancelled]);
    }

    #[test]
    fn other_input_does_not_exit() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1);
        assert!(matches!(
            p.handle(UiInput::Left, &mut ctx),
            ScreenChange::None
        ));
        assert!(ctx.exits.is_empty());
    }

    #[test]
    fn without_an_idle_trigger_tick_never_exits() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1);
        for _ in 0..100 {
            assert!(matches!(p.tick(&mut ctx), ScreenChange::None));
        }
        assert!(ctx.exits.is_empty());
    }

    #[test]
    fn the_idle_trigger_expires_into_idled() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1).with_idle(Countdown::new(2));
        p.tick(&mut ctx); // 1 of 2
        assert!(ctx.exits.is_empty(), "still waiting");
        p.tick(&mut ctx); // 2 of 2 -> idled
        assert_eq!(ctx.exits, [PromptExit::Idled]);
    }

    #[test]
    fn any_input_pushes_the_idle_trigger_back() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1).with_idle(Countdown::new(2));
        p.tick(&mut ctx); // 1 of 2
        p.handle(UiInput::Left, &mut ctx); // a sign of life: re-arm in full
        p.tick(&mut ctx); // 1 of 2 again — would have expired without the poke
        assert!(ctx.exits.is_empty(), "the poke pushed expiry back");
        p.tick(&mut ctx); // 2 of 2 -> idled
        assert_eq!(ctx.exits, [PromptExit::Idled]);
    }

    #[test]
    fn the_transition_fires_at_most_once() {
        // A confirm exits the card; the idle expiring afterwards (or another
        // press) must not fire the one-shot again.
        let mut ctx = Ctx::default();
        let mut p = prompt(1).with_idle(Countdown::new(1));
        p.handle(UiInput::Confirm, &mut ctx);
        p.tick(&mut ctx); // the idle expires, but the transition was taken
        p.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [PromptExit::Confirmed], "fires once");
    }

    #[test]
    fn collect_layers_contributes_every_banner_and_no_pixel_layer() {
        let ctx = Ctx::default();
        let p = prompt(3);
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        p.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(overlays.len(), 3);
        assert!(world.is_empty());
    }
}
