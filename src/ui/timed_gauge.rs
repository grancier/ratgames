//! [`TimedGauge`] — a [`Countdown`] driving a [`MeterBar`]: one advance per
//! frame drains the visible bar to the time still on the clock and reports
//! expiry exactly once.
//!
//! The arcade per-question clock: the game arms a frame budget and a bar (rect
//! and colours from its config), pumps [`advance`](TimedGauge::advance) each
//! frame it is answering, renders via
//! [`collect_layers`](TimedGauge::collect_layers), and acts on the single
//! `true` as time runs out (a timed-out miss). The gauge owns the binding the
//! game would otherwise hand-sync — countdown, bar fraction, and the fire-once
//! timeout — so the single-fire invariant is local rather than an emergent
//! property of the caller's phase interlock.
//!
//! [`with_seconds`](TimedGauge::with_seconds) adds a live digital readout of
//! the remaining whole seconds, exactly like
//! [`TimedCard::with_seconds`](crate::TimedCard::with_seconds): the game's bake
//! closure renders one displayed number (choosing its own style and screen
//! position), re-run only when the number changes.

use super::seconds_readout::SecondsReadout;
use super::{Countdown, MeterBar, ShadowBanner};
use crate::present::{OverlayLayer, PixelLayer};

/// A per-question clock: a [`Countdown`] frame budget bound to a draining
/// [`MeterBar`], with an optional digital seconds readout. Construct with
/// [`new`](Self::new), pump one [`advance`](Self::advance) per answering frame,
/// and contribute its layers with [`collect_layers`](Self::collect_layers).
pub struct TimedGauge<Ctx = ()> {
    countdown: Countdown,
    bar: MeterBar,
    fired: bool,
    seconds: Option<SecondsReadout<Ctx>>,
}

impl<Ctx> TimedGauge<Ctx> {
    /// A gauge over `countdown`'s budget, draining `bar` (built full by the
    /// game, positioned and coloured from its config).
    #[must_use]
    pub fn new(countdown: Countdown, bar: MeterBar) -> Self {
        Self {
            countdown,
            bar,
            fired: false,
            seconds: None,
        }
    }

    /// Show the remaining whole seconds as a live banner: `bake` renders one
    /// displayed number (it reads the game's `&Ctx` for its banner style and
    /// screen position) and is re-run only when the number changes. The readout
    /// counts the seconds still on the budget at `frames_per_second` (clamped
    /// to at least one), ceiled — a freshly-armed ten-second budget reads `10`.
    #[must_use]
    pub fn with_seconds(
        mut self,
        frames_per_second: u32,
        bake: impl Fn(u32, &Ctx) -> ShadowBanner + 'static,
    ) -> Self {
        self.seconds = Some(SecondsReadout::new(frames_per_second, bake));
        self
    }

    /// Advance one frame: drain the bar to the countdown's remaining fraction,
    /// refresh the readout, and report expiry — `true` exactly once, on the
    /// frame the budget runs out.
    pub fn advance(&mut self, ctx: &Ctx) -> bool {
        self.countdown.advance();
        self.bar
            .set_fraction(self.countdown.remaining(), self.countdown.total());
        if let Some(readout) = self.seconds.as_mut() {
            readout.sync(self.countdown.remaining(), ctx);
        }
        if self.countdown.is_expired() && !self.fired {
            self.fired = true;
            return true;
        }
        false
    }

    /// Frames left on the budget (zero once expired) — a time bonus reads this.
    #[must_use]
    pub fn remaining(&self) -> u32 {
        self.countdown.remaining()
    }

    /// The full frame budget the gauge was armed with.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.countdown.total()
    }

    /// The seconds the digital readout currently shows, or `None` before the
    /// first [`advance`](Self::advance) (or without
    /// [`with_seconds`](Self::with_seconds)).
    #[must_use]
    pub fn seconds_shown(&self) -> Option<u32> {
        self.seconds.as_ref().and_then(SecondsReadout::shown)
    }

    /// Contribute the gauge's layers: the bar into the pixel `world` (beneath
    /// the game's overlays) and the digital readout, once baked, into
    /// `overlays`.
    pub fn collect_layers<'a>(
        &'a self,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.bar);
        if let Some(readout) = &self.seconds
            && let Some(banner) = readout.banner()
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
    use crate::color::Color;
    use crate::geometry::{Point, Rect, Size};
    use crate::glyph::Bitmap8x8;
    use crate::ui::{ShadowBannerFactory, ShadowStyle};

    fn bar() -> MeterBar {
        MeterBar::new(
            Rect::new(Point::new(0, 0), Size::new(100, 4)),
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 255),
        )
    }

    fn gauge(frames: u32) -> TimedGauge {
        TimedGauge::new(Countdown::new(frames), bar())
    }

    #[test]
    fn advancing_drains_the_bar_with_the_clock() {
        let mut g = gauge(4);
        g.advance(&());
        assert_eq!(g.bar.fraction(), (3, 4));
        g.advance(&());
        assert_eq!(g.bar.fraction(), (2, 4));
        assert_eq!(g.remaining(), 2);
        assert_eq!(g.total(), 4);
    }

    #[test]
    fn expiry_fires_exactly_once() {
        let mut g = gauge(2);
        assert!(!g.advance(&()), "1 of 2 — still on the clock");
        assert!(g.advance(&()), "2 of 2 — the budget ran out this frame");
        assert!(!g.advance(&()), "already fired");
        assert!(!g.advance(&()), "stays fired");
        assert_eq!(g.bar.fraction(), (0, 2), "the bar stays drained");
    }

    #[test]
    fn a_zero_budget_fires_on_the_first_advance() {
        let mut g = gauge(0);
        assert!(g.advance(&()));
        assert!(!g.advance(&()));
    }

    /// A readout whose bake renders the number (and counts calls).
    fn seconds_gauge(frames: u32, fps: u32, bakes: Rc<Cell<u32>>) -> TimedGauge {
        gauge(frames).with_seconds(fps, move |secs, (): &()| {
            bakes.set(bakes.get() + 1);
            let factory =
                ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
            factory.at(&secs.to_string(), Point::new(0, 0), 1)
        })
    }

    #[test]
    fn the_seconds_readout_counts_whole_seconds_down() {
        let bakes = Rc::new(Cell::new(0));
        let mut g = seconds_gauge(180, 60, Rc::clone(&bakes)); // a 3-second budget
        assert_eq!(g.seconds_shown(), None, "nothing baked before an advance");

        g.advance(&()); // remaining 179 -> ceil(179/60) = 3
        assert_eq!(g.seconds_shown(), Some(3));
        for _ in 0..59 {
            g.advance(&()); // down to remaining 120 -> 2
        }
        assert_eq!(g.seconds_shown(), Some(2));
        // Two displayed numbers so far -> exactly two bakes, not one per frame.
        assert_eq!(bakes.get(), 2, "re-baked only when the number changes");
    }

    #[test]
    fn collect_layers_contributes_the_bar_and_the_baked_readout() {
        let bakes = Rc::new(Cell::new(0));
        let mut g = seconds_gauge(10, 1, bakes);
        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            g.collect_layers(&mut world, &mut overlays);
            assert_eq!(world.len(), 1, "the bar is always contributed");
            assert!(overlays.is_empty(), "no readout before the first advance");
        }
        g.advance(&());
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        g.collect_layers(&mut world, &mut overlays);
        assert_eq!(world.len(), 1);
        assert_eq!(overlays.len(), 1, "the baked readout rides the overlays");
    }

    #[test]
    fn without_a_readout_only_the_bar_is_contributed() {
        let mut g = gauge(5);
        g.advance(&());
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        g.collect_layers(&mut world, &mut overlays);
        assert_eq!(world.len(), 1);
        assert!(overlays.is_empty());
        assert_eq!(g.seconds_shown(), None);
    }
}
