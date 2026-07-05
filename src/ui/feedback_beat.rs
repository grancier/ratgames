//! [`FeedbackBeat`] — the arcade answer-feedback beat.
//!
//! The short "did I get it right?" moment after an answer: a two-phase overlay
//! sequence a game plays over its frozen challenge.
//!
//! * **Opening phase** (optional) — a [`Blink`] flashes a rejection glyph a few
//!   times (a red X on a miss). Skipped when there is no blink (a hit goes
//!   straight to the verdict).
//! * **Verdict phase** — a verdict [`ShadowBanner`] is held for a [`Countdown`],
//!   optionally under a [`Flash`] wash that fades out over the hold (a success
//!   tint). When the hold expires the beat is **done**.
//!
//! The beat owns the *timing and effects* — the blink, the wash and its fade, the
//! hold countdown, and the done signal — but **not** how its overlays sit among the
//! game's own layers. That ordering (a frozen problem behind the reject; a HUD
//! between the wash and the verdict) is app composition, not reusable UI policy, so
//! the beat is deliberately **not** an [`OverlayLayer`] and takes no problem / HUD
//! slots. Instead [`layers`](FeedbackBeat::layers) reports the phase-appropriate
//! pieces as a [`FeedbackBeatLayers`] and the caller composes them with its own.
//!
//! Like [`Blink`] / [`Flash`] / [`Countdown`], it owns a frame budget but not a
//! clock: the caller pumps one frame per [`advance`](FeedbackBeat::advance).

use super::{Blink, Countdown, Flash, ShadowBanner};
use crate::color::Color;

/// A success wash: the [`Flash`] drawn this frame and the full-strength colour it
/// fades from (kept because the flash's own colour changes as it fades).
struct Wash {
    flash: Flash,
    base: Color,
}

/// The overlays a [`FeedbackBeat`] wants drawn this frame, by phase — a render plan
/// the caller composites with its own layers (it decides what sits behind or
/// between them). Borrows the beat's overlays for the frame.
pub enum FeedbackBeatLayers<'a> {
    /// The opening phase: a rejection [`Blink`] flashing over the frozen challenge.
    Opening {
        /// The blinking rejection glyph (draws itself only on its lit frames).
        reject: &'a Blink,
    },
    /// The verdict phase: the verdict banner, optionally under a fading wash.
    Verdict {
        /// The success wash, present on a hit and fading over the hold; `None` on a
        /// miss.
        wash: Option<&'a Flash>,
        /// The verdict banner held for the countdown.
        verdict: &'a ShadowBanner,
    },
    /// The beat has finished; it contributes nothing (the caller resolves it).
    Done,
}

/// A two-phase answer-feedback beat: an optional opening [`Blink`], then a verdict
/// [`ShadowBanner`] held on a [`Countdown`] under an optional fading [`Flash`]
/// wash. Construct with [`new`](Self::new), pump one frame per
/// [`advance`](Self::advance), and read [`layers`](Self::layers) to render.
pub struct FeedbackBeat {
    /// The opening rejection blink; `None` once it finishes (or from the start on a
    /// hit) — its presence *is* the opening phase.
    reject: Option<Blink>,
    /// The success wash, faded over the hold; `None` on a miss.
    wash: Option<Wash>,
    verdict: ShadowBanner,
    hold: Countdown,
}

impl FeedbackBeat {
    /// A beat that opens with `reject` (a rejection blink, or `None` to skip
    /// straight to the verdict), then holds `verdict` for `hold`. `wash` is the
    /// full-strength colour of a success tint that fades over the hold, or `None`
    /// for no wash. The beat builds and fades the [`Flash`] itself.
    #[must_use]
    pub fn new(
        reject: Option<Blink>,
        wash: Option<Color>,
        verdict: ShadowBanner,
        hold: Countdown,
    ) -> Self {
        Self {
            reject,
            wash: wash.map(|base| Wash {
                flash: Flash::new(base),
                base,
            }),
            verdict,
            hold,
        }
    }

    /// Pump one frame: run the opening blink to completion, then (once it is done)
    /// fade the wash and run down the hold. Returns `true` on the frame the hold
    /// expires — the beat is now [`done`](Self::is_done).
    pub fn advance(&mut self) -> bool {
        // Opening phase: pump the blink; when it finishes, drop it so the verdict
        // phase begins next frame. The opening never completes the beat.
        if let Some(reject) = self.reject.as_mut() {
            reject.advance();
            if reject.is_done() {
                self.reject = None;
            }
            return false;
        }
        // Verdict phase: fade the wash off the hold's *current* remaining (before
        // advancing it), so the first held frame is full strength — the frame shape
        // the hand-rolled counter this replaces produced.
        if let Some(wash) = self.wash.as_mut() {
            wash.flash.set_color(
                wash.base
                    .scale_alpha(self.hold.remaining(), self.hold.total()),
            );
        }
        self.hold.advance();
        self.hold.is_expired()
    }

    /// Whether the beat has finished (the opening is over and the hold has
    /// expired).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.reject.is_none() && self.hold.is_expired()
    }

    /// The overlays to draw this frame, by phase (see [`FeedbackBeatLayers`]).
    #[must_use]
    pub fn layers(&self) -> FeedbackBeatLayers<'_> {
        if let Some(reject) = self.reject.as_ref() {
            FeedbackBeatLayers::Opening { reject }
        } else if self.hold.is_expired() {
            FeedbackBeatLayers::Done
        } else {
            FeedbackBeatLayers::Verdict {
                wash: self.wash.as_ref().map(|w| &w.flash),
                verdict: &self.verdict,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::palette;
    use crate::geometry::{Point, Size};
    use crate::glyph::Bitmap8x8;
    use crate::sprite::Sprite;
    use crate::ui::{BannerAnchor, ShadowBannerFactory, ShadowStyle};

    /// A 1x1 sprite for the reject blink (its shape is irrelevant to the beat).
    fn dot() -> Sprite {
        let mut s = Sprite::new(Size::new(1, 1));
        s.set(Point::ORIGIN, palette::FILL);
        s
    }

    fn verdict() -> ShadowBanner {
        let src = Bitmap8x8;
        ShadowBannerFactory::new(&src, ShadowStyle::default(), Size::new(64, 64)).centered("OK", 1)
    }

    fn reject_blink(blinks: u32, on: u32, off: u32) -> Blink {
        Blink::new(dot(), BannerAnchor::Center, Size::new(64, 64)).pattern(blinks, on, off)
    }

    fn wash_alpha(beat: &FeedbackBeat) -> Option<u8> {
        match beat.layers() {
            FeedbackBeatLayers::Verdict { wash, .. } => wash.map(|f| f.color().alpha()),
            _ => None,
        }
    }

    #[test]
    fn a_miss_runs_the_reject_blink_then_holds_the_verdict() {
        // Blink total = 1*(1+1) = 2 frames; hold = 3 frames.
        let mut beat = FeedbackBeat::new(
            Some(reject_blink(1, 1, 1)),
            None,
            verdict(),
            Countdown::new(3),
        );

        assert!(matches!(beat.layers(), FeedbackBeatLayers::Opening { .. }));
        assert!(!beat.advance()); // blink 1/2
        assert!(matches!(beat.layers(), FeedbackBeatLayers::Opening { .. }));
        assert!(!beat.advance()); // blink 2/2 -> reject dropped

        // Verdict phase, no wash on a miss.
        assert!(matches!(
            beat.layers(),
            FeedbackBeatLayers::Verdict { wash: None, .. }
        ));
        assert!(!beat.advance()); // hold 1/3
        assert!(!beat.advance()); // hold 2/3
        assert!(beat.advance()); // hold 3/3 -> done
        assert!(beat.is_done());
        assert!(matches!(beat.layers(), FeedbackBeatLayers::Done));
    }

    #[test]
    fn a_hit_holds_the_verdict_with_a_fading_wash() {
        let base = Color::argb(0x99, 0x39, 0xD3, 0x53);
        let mut beat = FeedbackBeat::new(None, Some(base), verdict(), Countdown::new(3));

        // No opening phase: straight to the verdict, wash at full strength.
        assert_eq!(wash_alpha(&beat), Some(0x99));
        assert!(!beat.advance()); // fades off remaining 3/3 -> still full
        assert_eq!(wash_alpha(&beat), Some(0x99));
        assert!(!beat.advance()); // fades off remaining 2/3
        assert!(
            wash_alpha(&beat).unwrap() < 0x99,
            "the wash fades over the hold"
        );
        assert!(beat.advance()); // remaining 1/3 -> hold expires -> done
        assert!(beat.is_done());
        assert!(matches!(beat.layers(), FeedbackBeatLayers::Done));
    }

    #[test]
    fn advance_reports_done_once_at_the_end_of_the_whole_beat() {
        // Blink total = 2*(2+2) = 8; hold = 4; the beat is 12 frames.
        let mut beat = FeedbackBeat::new(
            Some(reject_blink(2, 2, 2)),
            None,
            verdict(),
            Countdown::new(4),
        );
        let mut done_at = None;
        for frame in 1..=12 {
            if beat.advance() {
                done_at = Some(frame);
                break;
            }
        }
        assert_eq!(done_at, Some(12), "done on the final hold frame");
        assert!(beat.is_done());
    }
}
