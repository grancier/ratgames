//! Score and level progression — the pure state a session keeps around the
//! per-question [`Quiz`](crate::quiz::Quiz).

/// A running score in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Score(u32);

impl Score {
    /// A zero score.
    #[must_use]
    pub fn new() -> Self {
        Self(0)
    }

    /// The current points.
    #[must_use]
    pub fn points(self) -> u32 {
        self.0
    }

    /// Add `points`, saturating at [`u32::MAX`].
    pub fn add(&mut self, points: u32) {
        self.0 = self.0.saturating_add(points);
    }

    /// Reset back to zero.
    pub fn reset(&mut self) {
        self.0 = 0;
    }
}

/// Progress through an ordered set of levels, tracked by a 0-based index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelProgress {
    total: usize,
    current: usize,
}

impl LevelProgress {
    /// Progress over `total` levels, starting at the first.
    #[must_use]
    pub fn new(total: usize) -> Self {
        Self { total, current: 0 }
    }

    /// The total number of levels.
    #[must_use]
    pub fn total(self) -> usize {
        self.total
    }

    /// The current level index (`0..total`); equals `total` once complete.
    #[must_use]
    pub fn current(self) -> usize {
        self.current
    }

    /// Whether every level has been cleared.
    #[must_use]
    pub fn is_complete(self) -> bool {
        self.current >= self.total
    }

    /// Levels left to play, including the current one.
    #[must_use]
    pub fn remaining(self) -> usize {
        self.total.saturating_sub(self.current)
    }

    /// Advance to the next level. Returns whether a further level remains (i.e.
    /// the session is not yet complete); once complete it is a no-op returning
    /// `false`.
    pub fn advance(&mut self) -> bool {
        if self.is_complete() {
            return false;
        }
        self.current += 1;
        !self.is_complete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_adds_and_resets_and_saturates() {
        let mut s = Score::new();
        assert_eq!(s.points(), 0);
        s.add(10);
        s.add(5);
        assert_eq!(s.points(), 15);
        s.add(u32::MAX); // saturates, no overflow
        assert_eq!(s.points(), u32::MAX);
        s.reset();
        assert_eq!(s.points(), 0);
    }

    #[test]
    fn level_progress_advances_to_completion() {
        let mut p = LevelProgress::new(2);
        assert_eq!(p.current(), 0);
        assert_eq!(p.remaining(), 2);
        assert!(!p.is_complete());

        assert!(p.advance()); // -> level 1 of 2 (still a level to play)
        assert_eq!(p.current(), 1);
        assert_eq!(p.remaining(), 1);

        assert!(!p.advance()); // -> complete, no further level
        assert!(p.is_complete());
        assert_eq!(p.remaining(), 0);

        assert!(!p.advance()); // no-op once complete
        assert_eq!(p.current(), 2);
    }

    #[test]
    fn zero_levels_is_complete_immediately() {
        let mut p = LevelProgress::new(0);
        assert!(p.is_complete());
        assert!(!p.advance());
    }
}
