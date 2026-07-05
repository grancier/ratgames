//! [`Countdown`] — a frame-budget timer the caller pumps toward expiry.
//!
//! The arcade "hold N frames then move on" mechanic: an auto-advancing
//! interstitial card, a per-question time limit, a game-over continue timer. Like
//! [`Blink`](super::Blink) / [`Flash`](super::Flash) it owns a frame budget but
//! not a clock — the caller pumps one frame per [`advance`](Countdown::advance)
//! from its own frame source (e.g. `Screen::tick`) and checks
//! [`is_expired`](Countdown::is_expired) — so it is reusable across any pacing and
//! unit-testable with no timer.
//!
//! Unlike `Blink` / `Flash` it draws nothing: it is purely the timer, so a caller
//! pairs it with whatever it is timing (a card to auto-advance, a HUD clock read
//! from [`remaining`](Countdown::remaining), a time bonus).

/// A countdown of a fixed number of frames. Construct with [`new`](Countdown::new)
/// (or [`CountdownConfig::countdown`]), pump one frame per
/// [`advance`](Countdown::advance), and read [`is_expired`](Countdown::is_expired)
/// / [`remaining`](Countdown::remaining).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Countdown {
    total: u32,
    elapsed: u32,
}

impl Countdown {
    /// A countdown of `frames` frames. `frames == 0` is already expired — a
    /// zero-length hold, useful to disable an interstitial without special-casing.
    #[must_use]
    pub fn new(frames: u32) -> Self {
        Self {
            total: frames,
            elapsed: 0,
        }
    }

    /// Advance one frame, saturating at the budget so over-pumping stays expired.
    pub fn advance(&mut self) {
        if self.elapsed < self.total {
            self.elapsed += 1;
        }
    }

    /// Whether the budget has elapsed.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.elapsed >= self.total
    }

    /// Frames left before expiry (zero once expired). A HUD clock or a time bonus
    /// reads this.
    #[must_use]
    pub fn remaining(&self) -> u32 {
        self.total.saturating_sub(self.elapsed)
    }

    /// The full budget the countdown started with.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.total
    }

    /// Restart the countdown to its full budget.
    pub fn reset(&mut self) {
        self.elapsed = 0;
    }
}

/// A serde config for a [`Countdown`]: how many frames it runs. A game carries the
/// product value (an interstitial hold, a per-question time limit) in its config
/// and builds a fresh [`Countdown`] from it with [`countdown`](Self::countdown) —
/// the reusable *type* lives here, the *value* lives in the game's config, like
/// [`GameRules`](crate::GameRules) / [`AnswerMode`](crate::AnswerMode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CountdownConfig {
    /// Frames the countdown runs before expiring (`0` = no hold).
    pub frames: u32,
}

impl Default for CountdownConfig {
    fn default() -> Self {
        // A neutral ~1s hold at 60fps; a game carries its own value in config.
        Self { frames: 60 }
    }
}

impl CountdownConfig {
    /// A fresh [`Countdown`] of this config's `frames`.
    #[must_use]
    pub fn countdown(&self) -> Countdown {
        Countdown::new(self.frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_and_expires() {
        let mut c = Countdown::new(3);
        assert!(!c.is_expired());
        assert_eq!(c.remaining(), 3);
        assert_eq!(c.total(), 3);

        c.advance();
        assert_eq!(c.remaining(), 2);
        assert!(!c.is_expired());
        c.advance();
        c.advance();
        assert_eq!(c.remaining(), 0);
        assert!(c.is_expired());
    }

    #[test]
    fn over_pumping_saturates_at_expired() {
        let mut c = Countdown::new(1);
        for _ in 0..5 {
            c.advance();
        }
        assert!(c.is_expired());
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn zero_frames_is_expired_from_the_start() {
        let c = Countdown::new(0);
        assert!(c.is_expired());
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn reset_restores_the_full_budget() {
        let mut c = Countdown::new(2);
        c.advance();
        c.advance();
        assert!(c.is_expired());
        c.reset();
        assert!(!c.is_expired());
        assert_eq!(c.remaining(), 2);
    }

    #[test]
    fn config_builds_a_countdown_and_round_trips() {
        let config = CountdownConfig { frames: 90 };
        assert_eq!(config.countdown(), Countdown::new(90));

        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: CountdownConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);
        // A sparse config fills `frames` from the default.
        let defaulted: CountdownConfig = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, CountdownConfig::default());
    }
}
