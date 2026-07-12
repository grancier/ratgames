//! [`SecondsReadout`] — the shared live "9…8…7" seconds banner a
//! countdown-driven widget shows: the game's bake closure renders one displayed
//! number (reading the game's `&Ctx` for its banner style and screen position),
//! and the readout re-runs it only when the number changes. Used by
//! [`TimedCard`](crate::TimedCard) (a continue prompt's countdown) and
//! [`TimedGauge`](crate::TimedGauge) (a question clock's digital timer).

use super::ShadowBanner;

/// The bake for one displayed number: given the number and the game's context,
/// render the banner to show.
type SecondsBake<Ctx> = Box<dyn Fn(u32, &Ctx) -> ShadowBanner>;

/// A live seconds-remaining readout over a frame budget. The widget pumps
/// [`sync`](Self::sync) with the frames still left; the readout converts them to
/// whole seconds (ceiled, so a fresh ten-second budget reads `10` and the last
/// displayed second is `1`, never `0`) and re-bakes only on a change.
pub(crate) struct SecondsReadout<Ctx> {
    frames_per_second: u32,
    bake: SecondsBake<Ctx>,
    shown: Option<(u32, ShadowBanner)>,
}

impl<Ctx> SecondsReadout<Ctx> {
    /// A readout counting at `frames_per_second` (clamped to at least one).
    pub(crate) fn new(
        frames_per_second: u32,
        bake: impl Fn(u32, &Ctx) -> ShadowBanner + 'static,
    ) -> Self {
        Self {
            frames_per_second: frames_per_second.max(1),
            bake: Box::new(bake),
            shown: None,
        }
    }

    /// Re-bake if the displayed number changed for `remaining` frames left.
    pub(crate) fn sync(&mut self, remaining: u32, ctx: &Ctx) {
        let secs = remaining.div_ceil(self.frames_per_second);
        if self.shown.as_ref().map(|(shown, _)| *shown) != Some(secs) {
            self.shown = Some((secs, (self.bake)(secs, ctx)));
        }
    }

    /// The seconds currently displayed, or `None` before the first
    /// [`sync`](Self::sync).
    pub(crate) fn shown(&self) -> Option<u32> {
        self.shown.as_ref().map(|(secs, _)| *secs)
    }

    /// The baked banner to draw, or `None` before the first [`sync`](Self::sync).
    pub(crate) fn banner(&self) -> Option<&ShadowBanner> {
        self.shown.as_ref().map(|(_, banner)| banner)
    }
}
