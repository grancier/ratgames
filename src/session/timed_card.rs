//! [`TimedCard`] — a reusable interstitial screen: banners held for a
//! [`Countdown`], then gone.
//!
//! The arcade "show a card, then move on" screen: a level intro, a level-clear
//! tally, a game-over continue prompt. The card owns its banners and a countdown
//! and *is* a [`Screen`], so a game drops it straight onto the
//! [`ScreenStack`](crate::ScreenStack) without a per-screen boilerplate `impl` — it
//! auto-advances when the countdown expires, advances early on
//! [`Confirm`](UiInput::Confirm), and backs out on [`Cancel`](UiInput::Cancel).
//!
//! Where each of those exits *goes* is the game's business, not the toolkit's: the
//! card holds one [`FnOnce`] transition that maps the [`TimedCardExit`] reason to a
//! [`ScreenChange`]. So `ratgames` owns the timing and the rendering while the game
//! owns the routing. The transition runs with a fresh `&mut Ctx` at exit time
//! (rather than a prebuilt destination), so it reads current session state; and it
//! fires **once** — guarded by [`Option::take`] — so a confirm and an expiry on the
//! same frame cannot double-fire.

use super::{Screen, ScreenChange};
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::{Countdown, ShadowBanner, UiInput};

/// Why a [`TimedCard`] is leaving: the player confirmed, the hold elapsed, or the
/// player cancelled. The card's transition maps this to a [`ScreenChange`], so one
/// closure can treat confirm and expiry alike (the common auto-advance) yet still
/// fork on cancel (quit / back out) — or, later, distinguish all three (a
/// game-over continue prompt where confirm and time-out diverge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimedCardExit {
    /// The player pressed [`Confirm`](UiInput::Confirm) to skip the hold.
    Confirmed,
    /// The [`Countdown`] elapsed and the card auto-advanced.
    Expired,
    /// The player pressed [`Cancel`](UiInput::Cancel) to back out.
    Cancelled,
}

/// The one-shot transition a [`TimedCard`] fires as it leaves: given the exit
/// reason and mutable session state, it returns the stack change to apply.
type TimedCardTransition<Ctx> = Box<dyn FnOnce(TimedCardExit, &mut Ctx) -> ScreenChange<Ctx>>;

/// The bake for one displayed number of a seconds readout: given the number and
/// the game's context (for its banner style), render the banner to show.
type SecondsBake<Ctx> = Box<dyn Fn(u32, &Ctx) -> ShadowBanner>;

/// A live seconds-remaining readout on a [`TimedCard`] — the visible "9…8…7" of
/// a continue prompt. The game's `bake` closure renders one displayed number as
/// a banner; the card re-runs it only when the number changes.
struct SecondsDisplay<Ctx> {
    frames_per_second: u32,
    bake: SecondsBake<Ctx>,
    shown: Option<(u32, ShadowBanner)>,
}

/// A timed interstitial [`Screen`]: [`ShadowBanner`] overlays shown until a
/// [`Countdown`] expires (or the player confirms / cancels), then a single
/// transition routes onward. Construct with [`new`](Self::new) and push onto the
/// [`ScreenStack`](crate::ScreenStack); the game supplies the banners, the hold,
/// and where each exit leads. [`with_seconds`](Self::with_seconds) adds a live
/// countdown readout (a continue prompt's "9…8…7").
pub struct TimedCard<Ctx = ()> {
    overlays: Vec<ShadowBanner>,
    countdown: Countdown,
    on_exit: Option<TimedCardTransition<Ctx>>,
    seconds: Option<SecondsDisplay<Ctx>>,
    any_input_exits: bool,
}

impl<Ctx> TimedCard<Ctx> {
    /// A card showing `overlays` for `countdown`, routing each exit through
    /// `on_exit`. The transition fires at most once, with a fresh `&mut Ctx` — so
    /// it observes current session state and cannot double-fire.
    #[must_use]
    pub fn new(
        overlays: Vec<ShadowBanner>,
        countdown: Countdown,
        on_exit: impl FnOnce(TimedCardExit, &mut Ctx) -> ScreenChange<Ctx> + 'static,
    ) -> Self {
        Self {
            overlays,
            countdown,
            on_exit: Some(Box::new(on_exit)),
            seconds: None,
            any_input_exits: false,
        }
    }

    /// Exit as [`Confirmed`](TimedCardExit::Confirmed) on *any* input, not just
    /// Confirm — the attract-mode contract, where any sign of life wakes the
    /// game. [`Cancel`](UiInput::Cancel) still exits as
    /// [`Cancelled`](TimedCardExit::Cancelled), so the transition can keep its
    /// own routing for it.
    #[must_use]
    pub fn exit_on_any_input(mut self) -> Self {
        self.any_input_exits = true;
        self
    }

    /// Show the hold's remaining whole seconds as a live banner: `bake` renders
    /// one displayed number (it reads the game's `&Ctx` for its banner style) and
    /// is re-run only when the number changes. The readout counts the seconds
    /// still on the hold at `frames_per_second` (clamped to at least one), ceiled
    /// — a freshly-armed ten-second hold reads `10`, and the card exits as the
    /// readout would hit `0`.
    #[must_use]
    pub fn with_seconds(
        mut self,
        frames_per_second: u32,
        bake: impl Fn(u32, &Ctx) -> ShadowBanner + 'static,
    ) -> Self {
        self.seconds = Some(SecondsDisplay {
            frames_per_second: frames_per_second.max(1),
            bake: Box::new(bake),
            shown: None,
        });
        self
    }

    /// The seconds the live readout currently shows, or `None` before the first
    /// tick (or without [`with_seconds`](Self::with_seconds)).
    #[must_use]
    pub fn seconds_shown(&self) -> Option<u32> {
        self.seconds
            .as_ref()
            .and_then(|display| display.shown.as_ref().map(|(secs, _)| *secs))
    }

    /// Re-bake the seconds readout if the displayed number changed this frame.
    fn sync_seconds(&mut self, ctx: &Ctx) {
        let Some(display) = self.seconds.as_mut() else {
            return;
        };
        let secs = self
            .countdown
            .remaining()
            .div_ceil(display.frames_per_second);
        if display.shown.as_ref().map(|(shown, _)| *shown) != Some(secs) {
            display.shown = Some((secs, (display.bake)(secs, ctx)));
        }
    }

    /// Fire the one-shot transition for `why`, or [`ScreenChange::None`] if it has
    /// already fired — so an expiry after a confirm (or vice versa) is a no-op.
    fn exit(&mut self, why: TimedCardExit, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match self.on_exit.take() {
            Some(on_exit) => on_exit(why, ctx),
            None => ScreenChange::None,
        }
    }
}

impl<Ctx> Screen<Ctx> for TimedCard<Ctx> {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => self.exit(TimedCardExit::Confirmed, ctx),
            UiInput::Cancel => self.exit(TimedCardExit::Cancelled, ctx),
            _ if self.any_input_exits => self.exit(TimedCardExit::Confirmed, ctx),
            _ => ScreenChange::None,
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        self.countdown.advance();
        if self.countdown.is_expired() {
            return self.exit(TimedCardExit::Expired, ctx);
        }
        // The host ticks before it renders, so the readout is baked (and fresh)
        // for every frame the card shows — including the first.
        self.sync_seconds(ctx);
        ScreenChange::None
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
        if let Some(display) = &self.seconds
            && let Some((_, banner)) = &display.shown
        {
            overlays.push(banner);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::geometry::{Point, Size};
    use crate::glyph::Bitmap8x8;
    use crate::ui::{ShadowBannerFactory, ShadowStyle};

    /// Records each exit reason the card fires, proving the one-shot transition
    /// runs with `&mut Ctx` and the right [`TimedCardExit`].
    #[derive(Default)]
    struct Ctx {
        exits: Vec<TimedCardExit>,
    }

    /// `n` throwaway banners (their content is irrelevant to the card's mechanic).
    fn banners(n: usize) -> Vec<ShadowBanner> {
        let factory =
            ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
        (0..n)
            .map(|i| factory.at("HI", Point::new(0, i as i32), 1))
            .collect()
    }

    /// A card of `n` banners and a `frames`-frame hold that logs its exit reason.
    fn card(n: usize, frames: u32) -> TimedCard<Ctx> {
        TimedCard::new(banners(n), Countdown::new(frames), |why, ctx: &mut Ctx| {
            ctx.exits.push(why);
            ScreenChange::None
        })
    }

    #[test]
    fn confirm_exits_as_confirmed() {
        let mut ctx = Ctx::default();
        let mut c = card(1, 10);
        assert!(matches!(
            c.handle(UiInput::Confirm, &mut ctx),
            ScreenChange::None
        ));
        assert_eq!(ctx.exits, [TimedCardExit::Confirmed]);
    }

    #[test]
    fn cancel_exits_as_cancelled() {
        let mut ctx = Ctx::default();
        let mut c = card(1, 10);
        c.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [TimedCardExit::Cancelled]);
    }

    #[test]
    fn other_input_does_not_exit() {
        let mut ctx = Ctx::default();
        let mut c = card(1, 10);
        assert!(matches!(
            c.handle(UiInput::Left, &mut ctx),
            ScreenChange::None
        ));
        assert!(ctx.exits.is_empty());
    }

    #[test]
    fn tick_exits_as_expired_when_the_countdown_elapses() {
        let mut ctx = Ctx::default();
        let mut c = card(1, 2);
        assert!(matches!(c.tick(&mut ctx), ScreenChange::None)); // 1 of 2
        assert!(ctx.exits.is_empty(), "still holding");
        c.tick(&mut ctx); // 2 of 2 -> expired
        assert_eq!(ctx.exits, [TimedCardExit::Expired]);
    }

    #[test]
    fn the_transition_fires_at_most_once() {
        // A confirm exits the card; a later expiry (or another confirm) must not
        // fire the one-shot again — the guard against a same-frame double-exit.
        let mut ctx = Ctx::default();
        let mut c = card(1, 1);
        c.handle(UiInput::Confirm, &mut ctx);
        c.tick(&mut ctx); // countdown elapses, but the transition was already taken
        c.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.exits, [TimedCardExit::Confirmed], "fires once");
    }

    #[test]
    fn collect_layers_contributes_every_banner_and_no_pixel_layer() {
        let ctx = Ctx::default();
        let c = card(3, 5);
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        c.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(overlays.len(), 3);
        assert!(world.is_empty());
    }

    /// A seconds readout whose bake just renders the number (and counts calls).
    fn seconds_card(frames: u32, fps: u32, bakes: Rc<Cell<u32>>) -> TimedCard<Ctx> {
        card(1, frames).with_seconds(fps, move |secs, _ctx: &Ctx| {
            bakes.set(bakes.get() + 1);
            let factory =
                ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
            factory.at(&secs.to_string(), Point::new(0, 0), 1)
        })
    }

    #[test]
    fn the_seconds_readout_counts_whole_seconds_down() {
        let mut ctx = Ctx::default();
        let bakes = Rc::new(Cell::new(0));
        let mut c = seconds_card(180, 60, Rc::clone(&bakes)); // a 3-second hold
        assert_eq!(
            c.seconds_shown(),
            None,
            "nothing baked before the first tick"
        );

        c.tick(&mut ctx); // remaining 179 -> ceil(179/60) = 3
        assert_eq!(c.seconds_shown(), Some(3));

        for _ in 0..59 {
            c.tick(&mut ctx); // down to remaining 120 -> 2
        }
        assert_eq!(c.seconds_shown(), Some(2));
        // Two displayed numbers so far -> exactly two bakes, not one per frame.
        assert_eq!(bakes.get(), 2, "re-baked only when the number changes");

        for _ in 0..60 {
            c.tick(&mut ctx); // down to remaining 60 -> 1
        }
        assert_eq!(c.seconds_shown(), Some(1));
        assert!(ctx.exits.is_empty(), "still holding");

        for _ in 0..60 {
            c.tick(&mut ctx); // the hold elapses before a 0 ever shows
        }
        assert_eq!(ctx.exits, [TimedCardExit::Expired]);
        assert_eq!(
            c.seconds_shown(),
            Some(1),
            "the card left instead of showing 0"
        );
    }

    #[test]
    fn the_seconds_readout_renders_with_the_banners() {
        let mut ctx = Ctx::default();
        let mut c = seconds_card(120, 60, Rc::new(Cell::new(0)));
        c.tick(&mut ctx);
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        c.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(overlays.len(), 2, "the card's banner plus the readout");
    }

    #[test]
    fn a_zero_fps_readout_is_clamped_rather_than_dividing_by_zero() {
        let mut ctx = Ctx::default();
        let mut c = seconds_card(10, 0, Rc::new(Cell::new(0)));
        c.tick(&mut ctx); // clamped to 1 fps: remaining 9 -> 9
        assert_eq!(c.seconds_shown(), Some(9));
    }

    #[test]
    fn an_attract_card_wakes_on_any_input() {
        // Without the builder, a stray key does nothing (the earlier test); with
        // it, any sign of life exits as a confirm — and still only once.
        let mut ctx = Ctx::default();
        let mut c = card(1, 10).exit_on_any_input();
        c.handle(UiInput::Char('a'), &mut ctx);
        c.handle(UiInput::Left, &mut ctx);
        assert_eq!(ctx.exits, [TimedCardExit::Confirmed], "woke once");

        // Cancel keeps its own exit reason, so a game can still route it apart.
        let mut ctx = Ctx::default();
        let mut c = card(1, 10).exit_on_any_input();
        c.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [TimedCardExit::Cancelled]);
    }
}
