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

use super::{Blink, BlinkConfig, Countdown, Flash, ShadowBanner};
use crate::color::{Color, palette};

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

/// A serde config for a [`FeedbackBeat`]'s style and timing: the success-wash
/// and reject colours, how long the verdict holds, and the reject glyph's
/// magnification and blink pattern. A game carries the product values in its
/// config and builds each beat per attempt (the reject glyph itself comes from
/// the game's glyph source) — the reusable *type* lives here, like
/// [`CountdownConfig`](super::CountdownConfig).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FeedbackBeatConfig {
    /// Screen wash on a success (`#AARRGGBB`; the alpha is the strength).
    pub correct_color: Color,
    /// The reject glyph's colour on a miss (drawn solid, so alpha is moot).
    pub wrong_color: Color,
    /// How many frames the verdict holds before advancing.
    pub duration_frames: u32,
    /// Source-pixel magnification of the reject glyph.
    pub cross_scale: u32,
    /// The reject glyph's blink pattern.
    pub cross_blink: BlinkConfig,
}

impl Default for FeedbackBeatConfig {
    fn default() -> Self {
        // Neutral fallbacks: the toolkit palette for the colours (opaque,
        // generic) plus a plain half-second hold and 8× glyph. A game's tuned
        // values — a translucent wash, the exact hold and scale — live in its
        // config.
        Self {
            correct_color: palette::FILL,
            wrong_color: palette::DANGER,
            duration_frames: 30,
            cross_scale: 8,
            cross_blink: BlinkConfig {
                blinks: 3,
                on_frames: 12,
                off_frames: 12,
            },
        }
    }
}

/// Why a [`FeedbackBeatConfig`] was rejected: a value that would skip or hide
/// the feedback the beat exists to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FeedbackBeatConfigError {
    /// `duration_frames` was zero — the verdict would never hold.
    #[error("duration_frames must be at least 1")]
    ZeroDuration,
    /// `cross_scale` was zero — the reject glyph would vanish.
    #[error("cross_scale must be at least 1")]
    ZeroCrossScale,
    /// `cross_blink.blinks` was zero — a miss would never flash its cross.
    #[error("cross_blink.blinks must be at least 1")]
    ZeroBlinks,
    /// `cross_blink.on_frames` was zero — the cross would blink but never show.
    #[error("cross_blink.on_frames must be at least 1")]
    ZeroOnFrames,
}

impl FeedbackBeatConfig {
    /// A fresh verdict-hold countdown of this config's `duration_frames`.
    #[must_use]
    pub fn hold(&self) -> Countdown {
        Countdown::new(self.duration_frames)
    }

    /// Check the beat would actually show its feedback: a non-zero verdict
    /// hold, and a reject cross that is visible and lights up. (A bare
    /// [`BlinkConfig`] legitimately allows a never-lit pattern; a *reject
    /// opening* that never shows would silently swallow the miss feedback, so
    /// the beat rejects it here.)
    ///
    /// # Errors
    /// [`FeedbackBeatConfigError`] naming the first degenerate value found.
    pub fn validate(&self) -> Result<(), FeedbackBeatConfigError> {
        if self.duration_frames == 0 {
            return Err(FeedbackBeatConfigError::ZeroDuration);
        }
        if self.cross_scale == 0 {
            return Err(FeedbackBeatConfigError::ZeroCrossScale);
        }
        if self.cross_blink.blinks == 0 {
            return Err(FeedbackBeatConfigError::ZeroBlinks);
        }
        if self.cross_blink.on_frames == 0 {
            return Err(FeedbackBeatConfigError::ZeroOnFrames);
        }
        Ok(())
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

    #[test]
    fn config_validate_rejects_a_beat_that_would_show_nothing() {
        let base = FeedbackBeatConfig::default();
        assert!(base.validate().is_ok());
        assert_eq!(
            FeedbackBeatConfig {
                duration_frames: 0,
                ..base
            }
            .validate(),
            Err(FeedbackBeatConfigError::ZeroDuration)
        );
        assert_eq!(
            FeedbackBeatConfig {
                cross_scale: 0,
                ..base
            }
            .validate(),
            Err(FeedbackBeatConfigError::ZeroCrossScale)
        );
        assert_eq!(
            FeedbackBeatConfig {
                cross_blink: BlinkConfig {
                    blinks: 0,
                    ..base.cross_blink
                },
                ..base
            }
            .validate(),
            Err(FeedbackBeatConfigError::ZeroBlinks)
        );
        assert_eq!(
            FeedbackBeatConfig {
                cross_blink: BlinkConfig {
                    on_frames: 0,
                    ..base.cross_blink
                },
                ..base
            }
            .validate(),
            Err(FeedbackBeatConfigError::ZeroOnFrames)
        );
    }

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

    #[test]
    fn config_round_trips_and_arms_the_hold() {
        let config = FeedbackBeatConfig::default();
        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: FeedbackBeatConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);
        // A sparse config fills every field from the neutral default.
        let defaulted: FeedbackBeatConfig = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, FeedbackBeatConfig::default());
        assert_eq!(config.hold(), Countdown::new(config.duration_frames));
    }
}
