//! [`ContinuePrompt`] — the arcade game-over CONTINUE? flow: a timed card whose
//! exits carry continue semantics.
//!
//! The rendering and timing are exactly a [`TimedCard`] with a live seconds
//! readout; this type exists because the *flow* is a reusable arcade mechanism
//! worth naming. The player either chooses to continue
//! ([`Continued`](ContinueExit::Continued)), lets the countdown run out
//! ([`Declined`](ContinueExit::Declined)), or backs out
//! ([`Cancelled`](ContinueExit::Cancelled)) — and the game's one-shot
//! transition routes each typed exit. The continue *policy* — whether a
//! continue can actually be spent, what a declined run records — stays the
//! game's business inside that transition, alongside
//! [`ContinueRules`](super::ContinueRules) and the run it governs.

use super::timed_card::{TimedCard, TimedCardExit};
use super::{Screen, ScreenChange};
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::{Countdown, ShadowBanner, UiInput};

/// How the player left a [`ContinuePrompt`]: chose to continue, let the
/// countdown decline it, or backed out entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueExit {
    /// The player pressed [`Confirm`](UiInput::Confirm) — they want to spend a
    /// continue (whether one *can* be spent is the game's policy).
    Continued,
    /// The countdown ran out with no answer — the offer lapsed.
    Declined,
    /// The player pressed [`Cancel`](UiInput::Cancel).
    Cancelled,
}

/// The game-over CONTINUE? [`Screen`]: banners held on a countdown, a live
/// seconds readout ([`with_seconds`](Self::with_seconds)), and a single
/// one-shot transition routing the typed [`ContinueExit`]. Construct with
/// [`new`](Self::new) and push onto the [`ScreenStack`](crate::ScreenStack).
pub struct ContinuePrompt<Ctx = ()> {
    card: TimedCard<Ctx>,
}

impl<Ctx> ContinuePrompt<Ctx> {
    /// A prompt showing `overlays` for `countdown`, routing each exit through
    /// `on_exit`. The transition fires at most once, with a fresh `&mut Ctx`.
    #[must_use]
    pub fn new(
        overlays: Vec<ShadowBanner>,
        countdown: Countdown,
        on_exit: impl FnOnce(ContinueExit, &mut Ctx) -> ScreenChange<Ctx> + 'static,
    ) -> Self {
        Self {
            card: TimedCard::new(overlays, countdown, move |exit, ctx: &mut Ctx| {
                let exit = match exit {
                    TimedCardExit::Confirmed => ContinueExit::Continued,
                    TimedCardExit::Expired => ContinueExit::Declined,
                    TimedCardExit::Cancelled => ContinueExit::Cancelled,
                };
                on_exit(exit, ctx)
            }),
        }
    }

    /// Show the countdown's remaining whole seconds as a live banner — the
    /// prompt's "9…8…7". Delegates to
    /// [`TimedCard::with_seconds`](TimedCard::with_seconds): the game's `bake`
    /// renders one displayed number and re-runs only when it changes.
    #[must_use]
    pub fn with_seconds(
        mut self,
        frames_per_second: u32,
        bake: impl Fn(u32, &Ctx) -> ShadowBanner + 'static,
    ) -> Self {
        self.card = self.card.with_seconds(frames_per_second, bake);
        self
    }

    /// The seconds the live readout currently shows, or `None` before the
    /// first tick (or without [`with_seconds`](Self::with_seconds)).
    #[must_use]
    pub fn seconds_shown(&self) -> Option<u32> {
        self.card.seconds_shown()
    }
}

impl<Ctx> Screen<Ctx> for ContinuePrompt<Ctx> {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        self.card.handle(input, ctx)
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        self.card.tick(ctx)
    }

    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        self.card.collect_layers(ctx, world, overlays);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::glyph::Bitmap8x8;
    use crate::ui::{ShadowBannerFactory, ShadowStyle};

    /// Records each exit reason the prompt fires.
    #[derive(Default)]
    struct Ctx {
        exits: Vec<ContinueExit>,
    }

    fn banners(n: usize) -> Vec<ShadowBanner> {
        let factory =
            ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
        (0..n)
            .map(|i| factory.at("HI", Point::new(0, i as i32), 1))
            .collect()
    }

    fn prompt(frames: u32) -> ContinuePrompt<Ctx> {
        ContinuePrompt::new(banners(2), Countdown::new(frames), |why, ctx: &mut Ctx| {
            ctx.exits.push(why);
            ScreenChange::None
        })
    }

    #[test]
    fn confirm_exits_as_continued() {
        let mut ctx = Ctx::default();
        let mut p = prompt(10);
        p.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.exits, [ContinueExit::Continued]);
    }

    #[test]
    fn the_lapsed_countdown_exits_as_declined() {
        let mut ctx = Ctx::default();
        let mut p = prompt(2);
        p.tick(&mut ctx); // 1 of 2
        assert!(ctx.exits.is_empty(), "still offering");
        p.tick(&mut ctx); // 2 of 2 -> lapsed
        assert_eq!(ctx.exits, [ContinueExit::Declined]);
    }

    #[test]
    fn cancel_exits_as_cancelled() {
        let mut ctx = Ctx::default();
        let mut p = prompt(10);
        p.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [ContinueExit::Cancelled]);
    }

    #[test]
    fn the_transition_fires_at_most_once() {
        let mut ctx = Ctx::default();
        let mut p = prompt(1);
        p.handle(UiInput::Confirm, &mut ctx);
        p.tick(&mut ctx); // the countdown lapses, but the transition was taken
        p.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [ContinueExit::Continued], "fires once");
    }

    #[test]
    fn the_seconds_readout_rides_the_prompt() {
        let mut ctx = Ctx::default();
        let mut p = prompt(120).with_seconds(60, |secs, _ctx: &Ctx| {
            let factory =
                ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
            factory.at(&secs.to_string(), Point::new(0, 0), 1)
        });
        assert_eq!(p.seconds_shown(), None, "nothing baked before a tick");
        p.tick(&mut ctx); // remaining 119 -> ceil(119/60) = 2
        assert_eq!(p.seconds_shown(), Some(2));
    }

    #[test]
    fn collect_layers_delegates_to_the_card() {
        let ctx = Ctx::default();
        let p = prompt(10);
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        p.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(overlays.len(), 2);
        assert!(world.is_empty());
    }
}
