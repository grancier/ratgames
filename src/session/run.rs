//! Arcade run-state: the lives / 1-up / game-over loop layered over the
//! [`Score`] and [`LevelProgress`] a session already tracks.
//!
//! This is a pure, windowing-agnostic model of an 8-bit run — no rendering and
//! no math domain. A game feeds it three events (a level was cleared, an attempt
//! failed, a 1-up was collected) and reads back a [`RunPhase`] (`Playing`,
//! `GameOver`, or `Won`). Scoring policy and what counts as "cleared" stay with
//! the caller; this only sequences the loop.

use super::progress::{LevelProgress, Score};

/// A pool of arcade lives: lose one on a failed attempt, gain one from a 1-up,
/// game over at zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lives {
    current: u32,
    starting: u32,
}

impl Lives {
    /// A full pool of `starting` lives. `starting == 0` is game over from the
    /// outset.
    #[must_use]
    pub fn new(starting: u32) -> Self {
        Self {
            current: starting,
            starting,
        }
    }

    /// The lives remaining.
    #[must_use]
    pub fn count(self) -> u32 {
        self.current
    }

    /// The pool a fresh run begins with (and [`reset`](Lives::reset)s to).
    #[must_use]
    pub fn starting(self) -> u32 {
        self.starting
    }

    /// Whether no lives remain.
    #[must_use]
    pub fn is_game_over(self) -> bool {
        self.current == 0
    }

    /// Lose one life, saturating at zero. Returns whether the run continues
    /// (i.e. it is not yet game over).
    pub fn lose(&mut self) -> bool {
        self.current = self.current.saturating_sub(1);
        !self.is_game_over()
    }

    /// Gain one life from a 1-up, saturating at [`u32::MAX`]. A 1-up collected at
    /// zero revives the run.
    pub fn gain(&mut self) {
        self.current = self.current.saturating_add(1);
    }

    /// Refill to the starting pool for a new run.
    pub fn reset(&mut self) {
        self.current = self.starting;
    }
}

/// Where an arcade [`Run`] stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    /// Lives remain and levels are left to clear.
    Playing,
    /// Out of lives.
    GameOver,
    /// Every level cleared.
    Won,
}

/// An arcade run: a [`Score`], a pool of [`Lives`], and [`LevelProgress`], driven
/// by three events — level cleared, attempt failed, 1-up collected.
///
/// The caller owns policy: how many points an answer is worth and what counts as
/// clearing a level. `Run` only sequences the loop and reports the [`RunPhase`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    score: Score,
    lives: Lives,
    levels: LevelProgress,
}

impl Run {
    /// A new run over `levels` levels with `lives` lives and a zero score.
    #[must_use]
    pub fn new(lives: u32, levels: usize) -> Self {
        Self {
            score: Score::new(),
            lives: Lives::new(lives),
            levels: LevelProgress::new(levels),
        }
    }

    /// The running score.
    #[must_use]
    pub fn score(self) -> Score {
        self.score
    }

    /// The lives pool.
    #[must_use]
    pub fn lives(self) -> Lives {
        self.lives
    }

    /// Progress through the levels.
    #[must_use]
    pub fn levels(self) -> LevelProgress {
        self.levels
    }

    /// The current phase: game over once out of lives, won once every level is
    /// cleared, otherwise still playing. A dead run is never "won".
    #[must_use]
    pub fn phase(self) -> RunPhase {
        if self.lives.is_game_over() {
            RunPhase::GameOver
        } else if self.levels.is_complete() {
            RunPhase::Won
        } else {
            RunPhase::Playing
        }
    }

    /// Award `points`.
    pub fn award(&mut self, points: u32) {
        self.score.add(points);
    }

    /// Clear the current level and advance. Returns the resulting phase.
    pub fn clear_level(&mut self) -> RunPhase {
        self.levels.advance();
        self.phase()
    }

    /// Register a failed attempt: lose a life. Returns the resulting phase.
    pub fn fail(&mut self) -> RunPhase {
        self.lives.lose();
        self.phase()
    }

    /// Collect a 1-up: gain a life (reviving the run if it was game over).
    pub fn one_up(&mut self) {
        self.lives.gain();
    }

    /// Restart: zero score, refill lives, back to the first level.
    pub fn reset(&mut self) {
        self.score.reset();
        self.lives.reset();
        self.levels = LevelProgress::new(self.levels.total());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lives_lose_down_to_game_over() {
        let mut lives = Lives::new(3);
        assert_eq!(lives.count(), 3);
        assert!(!lives.is_game_over());
        assert!(lives.lose()); // -> 2, run continues
        assert!(lives.lose()); // -> 1
        assert!(!lives.lose()); // -> 0, game over
        assert!(lives.is_game_over());
        assert!(!lives.lose()); // saturates at zero
        assert_eq!(lives.count(), 0);
    }

    #[test]
    fn a_one_up_revives_a_dead_pool_and_reset_refills() {
        let mut lives = Lives::new(1);
        assert!(!lives.lose()); // -> 0
        assert!(lives.is_game_over());
        lives.gain(); // 1-up revives
        assert_eq!(lives.count(), 1);
        assert!(!lives.is_game_over());
        lives.gain(); // -> 2, can exceed the starting pool
        assert_eq!(lives.count(), 2);
        lives.reset();
        assert_eq!(lives.count(), 1);
        assert_eq!(lives.starting(), 1);
    }

    #[test]
    fn zero_lives_is_game_over_immediately() {
        assert!(Lives::new(0).is_game_over());
    }

    #[test]
    fn run_plays_through_levels_to_a_win() {
        let mut run = Run::new(3, 2);
        assert_eq!(run.phase(), RunPhase::Playing);
        assert_eq!(run.clear_level(), RunPhase::Playing); // -> level 1 of 2
        assert_eq!(run.levels().current(), 1);
        assert_eq!(run.clear_level(), RunPhase::Won); // last level cleared
        assert!(run.levels().is_complete());
    }

    #[test]
    fn run_reaches_game_over_when_lives_run_out() {
        let mut run = Run::new(1, 3);
        assert_eq!(run.fail(), RunPhase::GameOver);
        assert!(run.lives().is_game_over());
        // A 1-up continues the run from where it left off.
        run.one_up();
        assert_eq!(run.phase(), RunPhase::Playing);
    }

    #[test]
    fn run_awards_points_and_resets() {
        let mut run = Run::new(3, 2);
        run.award(10);
        run.award(5);
        run.fail();
        run.clear_level();
        assert_eq!(run.score().points(), 15);

        run.reset();
        assert_eq!(run.score().points(), 0);
        assert_eq!(run.lives().count(), 3);
        assert_eq!(run.levels().current(), 0);
        assert_eq!(run.levels().total(), 2);
    }

    #[test]
    fn game_over_takes_precedence_over_an_unfinished_run() {
        // Out of lives on the final level, before clearing it: game over, not won.
        let mut run = Run::new(1, 1);
        assert_eq!(run.fail(), RunPhase::GameOver);
    }
}
