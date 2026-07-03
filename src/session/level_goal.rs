//! A level clear/fail goal: enough successes before too many failures.
//!
//! `LevelGoal` is generic arcade machinery — it counts successes and failures
//! toward fixed thresholds and reports whether the current level is still in
//! progress, cleared, or failed. It carries no scoring, lives, banners, or
//! level advancement (those belong to [`Run`](super::Run) and the game layer);
//! a game feeds it a stream of success/failure outcomes and drives the rest
//! from the [`LevelOutcome`]. The vocabulary is deliberately neutral (success /
//! failure, not correct / miss) so any game — not just a quiz — can reuse it.

/// Why a [`LevelGoal`] was rejected at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelGoalError {
    /// `required_successes` was zero — a level must need at least one success to
    /// be clearable. Rejected rather than silently treated as already cleared.
    ZeroRequired,
}

/// Where a [`LevelGoal`] stands given the successes and failures recorded so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelOutcome {
    /// Neither threshold reached yet.
    InProgress,
    /// Reached `required_successes` — the level is cleared.
    Cleared,
    /// Exceeded `max_failures` — the level is failed.
    Failed,
}

/// A per-level clear/fail goal: cleared by reaching `required_successes`, failed
/// once failures exceed `max_failures`.
///
/// Feed outcomes with [`record`](LevelGoal::record). The first terminal
/// [`LevelOutcome`] is **sticky** — once cleared or failed, further records are
/// ignored and both the outcome and the counts stay put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelGoal {
    required_successes: u32,
    max_failures: u32,
    successes: u32,
    failures: u32,
}

impl LevelGoal {
    /// A goal cleared at `required_successes` and failed once failures exceed
    /// `max_failures`.
    ///
    /// `max_failures` of zero is valid — it means the first failure fails the
    /// level. `required_successes` must be non-zero, else
    /// [`LevelGoalError::ZeroRequired`].
    pub fn new(required_successes: u32, max_failures: u32) -> Result<Self, LevelGoalError> {
        if required_successes == 0 {
            return Err(LevelGoalError::ZeroRequired);
        }
        Ok(Self {
            required_successes,
            max_failures,
            successes: 0,
            failures: 0,
        })
    }

    /// Successes required to clear the level.
    #[must_use]
    pub fn required_successes(self) -> u32 {
        self.required_successes
    }

    /// Failures tolerated before the level fails (one more than this fails it).
    #[must_use]
    pub fn max_failures(self) -> u32 {
        self.max_failures
    }

    /// Successes recorded so far.
    #[must_use]
    pub fn successes(self) -> u32 {
        self.successes
    }

    /// Failures recorded so far.
    #[must_use]
    pub fn failures(self) -> u32 {
        self.failures
    }

    /// The current outcome for the recorded counts.
    #[must_use]
    pub fn outcome(self) -> LevelOutcome {
        if self.successes >= self.required_successes {
            LevelOutcome::Cleared
        } else if self.failures > self.max_failures {
            LevelOutcome::Failed
        } else {
            LevelOutcome::InProgress
        }
    }

    /// Record one success or failure and return the resulting [`LevelOutcome`].
    ///
    /// Once the goal is terminal (cleared or failed) the outcome is sticky: the
    /// record is ignored and the existing outcome is returned unchanged.
    pub fn record(&mut self, success: bool) -> LevelOutcome {
        let current = self.outcome();
        if current != LevelOutcome::InProgress {
            return current;
        }
        if success {
            self.successes += 1;
        } else {
            self.failures += 1;
        }
        self.outcome()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_zero_required_successes() {
        assert_eq!(LevelGoal::new(0, 3), Err(LevelGoalError::ZeroRequired));
    }

    #[test]
    fn new_accepts_zero_max_failures() {
        // Zero tolerated failures is a valid (strict) goal, not an error.
        let goal = LevelGoal::new(1, 0).unwrap();
        assert_eq!(goal.max_failures(), 0);
        assert_eq!(goal.outcome(), LevelOutcome::InProgress);
    }

    #[test]
    fn clears_on_reaching_required_successes() {
        let mut goal = LevelGoal::new(3, 2).unwrap();
        assert_eq!(goal.record(true), LevelOutcome::InProgress);
        assert_eq!(goal.record(true), LevelOutcome::InProgress);
        assert_eq!(goal.record(true), LevelOutcome::Cleared);
        assert_eq!(goal.successes(), 3);
    }

    #[test]
    fn fails_once_failures_exceed_max() {
        let mut goal = LevelGoal::new(5, 1).unwrap();
        assert_eq!(goal.record(false), LevelOutcome::InProgress); // 1 failure == max, still ok
        assert_eq!(goal.record(false), LevelOutcome::Failed); // 2 > 1
        assert_eq!(goal.failures(), 2);
    }

    #[test]
    fn zero_max_failures_fails_on_first_failure() {
        let mut goal = LevelGoal::new(3, 0).unwrap();
        assert_eq!(goal.record(false), LevelOutcome::Failed);
    }

    #[test]
    fn interleaved_successes_and_failures_can_still_clear() {
        let mut goal = LevelGoal::new(3, 2).unwrap();
        assert_eq!(goal.record(true), LevelOutcome::InProgress); // s1
        assert_eq!(goal.record(false), LevelOutcome::InProgress); // f1
        assert_eq!(goal.record(true), LevelOutcome::InProgress); // s2
        assert_eq!(goal.record(false), LevelOutcome::InProgress); // f2 (== max, ok)
        assert_eq!(goal.record(true), LevelOutcome::Cleared); // s3
    }

    #[test]
    fn cleared_is_sticky_and_freezes_counts() {
        let mut goal = LevelGoal::new(1, 5).unwrap();
        assert_eq!(goal.record(true), LevelOutcome::Cleared);
        // Further records are ignored once terminal.
        assert_eq!(goal.record(false), LevelOutcome::Cleared);
        assert_eq!(goal.record(true), LevelOutcome::Cleared);
        assert_eq!(goal.successes(), 1);
        assert_eq!(goal.failures(), 0);
    }

    #[test]
    fn failed_is_sticky_and_freezes_counts() {
        let mut goal = LevelGoal::new(3, 0).unwrap();
        assert_eq!(goal.record(false), LevelOutcome::Failed);
        assert_eq!(goal.record(true), LevelOutcome::Failed);
        assert_eq!(goal.successes(), 0);
        assert_eq!(goal.failures(), 1);
    }

    #[test]
    fn accessors_report_thresholds_and_counts() {
        let goal = LevelGoal::new(4, 2).unwrap();
        assert_eq!(goal.required_successes(), 4);
        assert_eq!(goal.max_failures(), 2);
        assert_eq!(goal.successes(), 0);
        assert_eq!(goal.failures(), 0);
        assert_eq!(goal.outcome(), LevelOutcome::InProgress);
    }
}
