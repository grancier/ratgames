//! [`MeterBar`] — a horizontal bar whose fill length shows a fraction (a
//! [`PixelLayer`]).
//!
//! The arcade gauge: a per-question time bar, a health / charge / boss-HP bar, a
//! lesson-progress meter. It draws a solid `track_color` rectangle (the empty
//! channel), then a `fill_color` rectangle across the left `numerator /
//! denominator` of its width — so a shrinking fraction reads as a bar draining
//! left-to-right, a growing one as it fills. It is crisp integer pixel art, so it
//! is a [`PixelLayer`] drawn into the virtual screen and integer-upscaled with
//! everything else — no anti-aliasing, no device-space projection.
//!
//! The fraction is an **integer ratio** (`numerator / denominator`), never a
//! float: the caller maps whatever it is metering — a
//! [`Countdown`](super::Countdown)'s `remaining` / `total`, current / max health —
//! into the ratio, and the bar clamps `numerator` to `denominator` and treats a
//! zero `denominator` as empty, so there is no NaN, no over-fill past the rect, and
//! no panic. It holds no clock and no `Countdown`; it is purely the gauge, reusable
//! across any bounded quantity without taking on the meaning of what it shows.

use crate::color::{Color, palette};
use crate::geometry::{Rect, Size};
use crate::present::PixelLayer;
use crate::surface::Surface;

/// A horizontal bar filled left-to-right to `numerator / denominator` of its
/// `rect`, in `fill_color` over a `track_color` back.
///
/// Construct with [`new`](MeterBar::new) (full by default), set the fraction with
/// [`with_fraction`](MeterBar::with_fraction) / [`set_fraction`](MeterBar::set_fraction),
/// and push it into the pixel `world`. A [`Color::TRANSPARENT`] `track_color` draws
/// no track (the fill floats over the backdrop); likewise a transparent
/// `fill_color` draws no fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterBar {
    rect: Rect,
    fill_color: Color,
    track_color: Color,
    numerator: u32,
    denominator: u32,
}

impl MeterBar {
    /// A bar occupying `rect`, its fill in `fill_color` over a `track_color` back.
    /// Starts full (`1 / 1`); set the fraction with
    /// [`with_fraction`](Self::with_fraction) / [`set_fraction`](Self::set_fraction).
    #[must_use]
    pub fn new(rect: Rect, fill_color: Color, track_color: Color) -> Self {
        Self {
            rect,
            fill_color,
            track_color,
            numerator: 1,
            denominator: 1,
        }
    }

    /// Set the fill fraction at construction — the builder form of
    /// [`set_fraction`](Self::set_fraction).
    #[must_use]
    pub fn with_fraction(mut self, numerator: u32, denominator: u32) -> Self {
        self.set_fraction(numerator, denominator);
        self
    }

    /// Set the fill fraction to `numerator / denominator`. A `numerator` above
    /// `denominator` renders as full (never over-fills the rect) and a zero
    /// `denominator` renders as empty; the values are stored verbatim (clamping is
    /// a render-time concern), so [`fraction`](Self::fraction) reports back exactly
    /// what was set.
    pub fn set_fraction(&mut self, numerator: u32, denominator: u32) {
        self.numerator = numerator;
        self.denominator = denominator;
    }

    /// The current `(numerator, denominator)` fraction, as last set.
    #[must_use]
    pub fn fraction(&self) -> (u32, u32) {
        (self.numerator, self.denominator)
    }

    /// The fill width in pixels: the left `numerator / denominator` of the track
    /// width, with `numerator` clamped to `denominator` and a zero `denominator`
    /// yielding zero. Computed in `u64` so a wide track times a large numerator
    /// cannot overflow; the clamp keeps the result `<= rect.size.w`, so the cast
    /// back to `u32` is lossless.
    fn fill_width(&self) -> u32 {
        if self.denominator == 0 {
            return 0;
        }
        let numerator = self.numerator.min(self.denominator);
        let width =
            u64::from(self.rect.size.w) * u64::from(numerator) / u64::from(self.denominator);
        width as u32
    }
}

impl PixelLayer for MeterBar {
    fn render(&self, screen: &mut Surface) {
        // Track first (the whole channel), then the fill over its left portion, so
        // the track shows through wherever the bar has drained.
        screen.fill_rect(self.rect, self.track_color);
        let fill = self.fill_width();
        if fill > 0 {
            screen.fill_rect(
                Rect::new(self.rect.origin, Size::new(fill, self.rect.size.h)),
                self.fill_color,
            );
        }
    }
}

/// A serde config for a [`MeterBar`]'s colours: the draining fill and the
/// track behind it. A game carries the product values in its config and builds
/// the bar with [`bar`](Self::bar) — the reusable *type* lives here, the
/// *values* live in the game's config, like
/// [`CountdownConfig`](super::CountdownConfig).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MeterBarConfig {
    /// The fill colour — the portion still on the meter.
    pub fill_color: Color,
    /// The track colour behind the fill — the drained / empty channel. A
    /// transparent colour shows the backdrop through the drained portion
    /// instead.
    pub track_color: Color,
}

impl Default for MeterBarConfig {
    fn default() -> Self {
        // Neutral fallbacks from the toolkit palette (amber fill over a
        // near-black channel); a game's tuned colours live in its config.
        Self {
            fill_color: palette::WARNING,
            track_color: palette::PANEL,
        }
    }
}

impl MeterBarConfig {
    /// A full [`MeterBar`] occupying `rect` in this config's colours.
    #[must_use]
    pub fn bar(&self, rect: Rect) -> MeterBar {
        MeterBar::new(rect, self.fill_color, self.track_color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    const FILL: Color = Color::rgb(0x11, 0x22, 0x33);
    const TRACK: Color = Color::rgb(0x44, 0x55, 0x66);
    const BG: Color = Color::rgb(0, 0, 0);

    fn word_at(s: &Surface, x: u32, y: u32) -> u32 {
        s.as_slice()[y as usize * s.size().w as usize + x as usize]
    }

    /// Render a `w`×1 bar at `num / den` over the whole surface and return it.
    fn bar_row(w: u32, num: u32, den: u32) -> Surface {
        let mut s = Surface::new(Size::new(w, 1), BG);
        MeterBar::new(Rect::from_size(Size::new(w, 1)), FILL, TRACK)
            .with_fraction(num, den)
            .render(&mut s);
        s
    }

    #[test]
    fn a_full_bar_fills_the_whole_track() {
        // The default fraction (1 / 1) fills the whole rect.
        let mut s = Surface::new(Size::new(4, 1), BG);
        MeterBar::new(Rect::from_size(Size::new(4, 1)), FILL, TRACK).render(&mut s);
        for x in 0..4 {
            assert_eq!(word_at(&s, x, 0), FILL.packed());
        }
    }

    #[test]
    fn a_partial_bar_fills_the_left_portion_over_the_track() {
        let s = bar_row(10, 3, 10);
        for x in 0..3 {
            assert_eq!(word_at(&s, x, 0), FILL.packed(), "left 3/10 is filled");
        }
        for x in 3..10 {
            assert_eq!(
                word_at(&s, x, 0),
                TRACK.packed(),
                "the rest shows the track"
            );
        }
    }

    #[test]
    fn numerator_above_denominator_clamps_to_full() {
        // 7/4 renders as full, not 175% (which would over-fill the rect).
        let s = bar_row(4, 7, 4);
        for x in 0..4 {
            assert_eq!(word_at(&s, x, 0), FILL.packed());
        }
    }

    #[test]
    fn a_zero_denominator_reads_as_empty_not_a_panic() {
        let s = bar_row(4, 3, 0);
        for x in 0..4 {
            assert_eq!(word_at(&s, x, 0), TRACK.packed(), "only the track shows");
        }
    }

    #[test]
    fn an_empty_bar_shows_only_the_track() {
        let s = bar_row(4, 0, 4);
        for x in 0..4 {
            assert_eq!(word_at(&s, x, 0), TRACK.packed());
        }
    }

    #[test]
    fn fraction_round_trips_what_was_set_even_out_of_range() {
        let mut bar = MeterBar::new(Rect::from_size(Size::new(4, 1)), FILL, TRACK);
        assert_eq!(bar.fraction(), (1, 1)); // full by default
        bar.set_fraction(7, 4);
        assert_eq!(bar.fraction(), (7, 4)); // stored verbatim; clamping is at render
        bar.set_fraction(3, 0);
        assert_eq!(bar.fraction(), (3, 0));
    }

    #[test]
    fn with_fraction_matches_new_then_set_fraction() {
        let rect = Rect::from_size(Size::new(8, 2));
        let built = MeterBar::new(rect, FILL, TRACK).with_fraction(3, 8);
        let mut mutated = MeterBar::new(rect, FILL, TRACK);
        mutated.set_fraction(3, 8);
        assert_eq!(built, mutated);
    }

    #[test]
    fn the_bar_honours_its_rect_origin_and_height() {
        // A bar offset from the origin fills only its own rows and columns, leaving
        // the surrounding screen untouched.
        let mut s = Surface::new(Size::new(6, 4), BG);
        let rect = Rect::new(Point::new(1, 1), Size::new(4, 2));
        MeterBar::new(rect, FILL, TRACK)
            .with_fraction(1, 2)
            .render(&mut s);
        // Rows above and below the bar are untouched.
        for x in 0..6 {
            assert_eq!(word_at(&s, x, 0), BG.packed());
            assert_eq!(word_at(&s, x, 3), BG.packed());
        }
        // In the bar's rows: col 0 and col 5 are outside it; cols 1–2 are the filled
        // half (width 4 × 1/2 = 2 px), cols 3–4 the drained track.
        for y in 1..3 {
            assert_eq!(word_at(&s, 0, y), BG.packed());
            assert_eq!(word_at(&s, 1, y), FILL.packed());
            assert_eq!(word_at(&s, 2, y), FILL.packed());
            assert_eq!(word_at(&s, 3, y), TRACK.packed());
            assert_eq!(word_at(&s, 4, y), TRACK.packed());
            assert_eq!(word_at(&s, 5, y), BG.packed());
        }
    }

    #[test]
    fn a_transparent_track_leaves_the_backdrop_between_fill_and_edge() {
        // With no track colour the drained portion shows the backdrop, not a track.
        let mut s = Surface::new(Size::new(4, 1), BG);
        MeterBar::new(Rect::from_size(Size::new(4, 1)), FILL, Color::TRANSPARENT)
            .with_fraction(1, 4)
            .render(&mut s);
        assert_eq!(word_at(&s, 0, 0), FILL.packed());
        for x in 1..4 {
            assert_eq!(word_at(&s, x, 0), BG.packed(), "no track drawn");
        }
    }

    #[test]
    fn config_round_trips_and_builds_a_bar() {
        let config = MeterBarConfig::default();
        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: MeterBarConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);
        // A sparse config fills both colours from the neutral default.
        let defaulted: MeterBarConfig = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, MeterBarConfig::default());

        let bar = config.bar(Rect::new(
            crate::geometry::Point::new(2, 3),
            Size::new(10, 2),
        ));
        assert_eq!(bar.fraction(), (1, 1), "built full");
    }
}
